//! X11 window host built directly on the X protocol (x11rb).
//!
//! One window per [`run`] call. Events are read from the X connection and a
//! wake pipe with `poll(2)`, so an idle app costs no CPU: there is no polling
//! interval. Frames are software-rendered and uploaded with `PutImage`.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection};
use x11rb::properties::WmSizeHints;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, CreateGCAux, CreateWindowAux,
    EventMask, GetModifierMappingReply, ImageFormat, ImageOrder, PropMode, WindowClass,
};
use x11rb::protocol::Event as XEvent;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use super::keys::{key_for, keysym_for, modifiers};
use super::{scale_from_environment, App, Control, Edge, WindowKind, WindowOptions};
use crate::event::{AccessibilityRequest, Event, PointerButton};
use crate::render::RasterSurface;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        UTF8_STRING,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_STRUT,
        _NET_WM_STRUT_PARTIAL,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("cannot connect to the X server: {0}")]
    Connect(#[from] x11rb::errors::ConnectError),
    #[error("X11 connection failed: {0}")]
    Connection(#[from] x11rb::errors::ConnectionError),
    #[error("X11 request failed: {0}")]
    Reply(#[from] x11rb::errors::ReplyError),
    #[error("X11 resource allocation failed: {0}")]
    Id(#[from] x11rb::errors::ReplyOrIdError),
    #[error("the X server has no 32-bit TrueColor pixmap format for depth {0}")]
    UnsupportedVisual(u8),
    #[error(transparent)]
    Render(#[from] crate::render::RenderError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Wakes the event loop from another thread; the app receives [`Event::Wake`].
#[derive(Clone)]
pub struct Proxy {
    write: Arc<OwnedFd>,
}

impl Proxy {
    pub fn wake(&self) {
        // A full pipe already guarantees a pending wakeup.
        // SAFETY: writing one byte from a valid buffer to an owned fd.
        unsafe {
            libc::write(self.write.as_raw_fd(), [1u8].as_ptr().cast(), 1);
        }
    }
}

struct WakePipe {
    read: OwnedFd,
    proxy: Proxy,
}

impl WakePipe {
    fn new() -> std::io::Result<Self> {
        let mut fds = [0; 2];
        // SAFETY: pipe2 fills two valid descriptors on success.
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: the descriptors were just created and are owned here.
        let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        Ok(Self {
            read,
            proxy: Proxy {
                write: Arc::new(write),
            },
        })
    }

    /// Drain pending wake bytes. Returns whether the pipe had been signalled.
    fn drain(&self) -> bool {
        let mut buffer = [0u8; 64];
        let mut signalled = false;
        // SAFETY: reading into a local buffer from an owned non-blocking fd.
        while unsafe {
            libc::read(
                self.read.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        } > 0
        {
            signalled = true;
        }
        signalled
    }
}

/// Messages from the AccessKit adapter thread.
enum A11yMessage {
    Activated,
    Request(AccessibilityRequest),
}

/// Run `app` in a new window until it quits. `on_proxy` receives a [`Proxy`]
/// before the loop starts, for threads that need to wake the app.
pub fn run<A: App>(
    options: WindowOptions,
    mut app: A,
    on_proxy: impl FnOnce(Proxy),
) -> Result<(), PlatformError> {
    let (conn, screen_number) = x11rb::connect(None)?;
    let screen = conn.setup().roots[screen_number].clone();
    let atoms = Atoms::new(&conn)?.reply()?;
    let scale = options.scale.unwrap_or_else(scale_from_environment);
    let factor = scale.factor();

    let format = conn
        .setup()
        .pixmap_formats
        .iter()
        .find(|format| format.depth == screen.root_depth)
        .copied();
    if format.map(|format| format.bits_per_pixel) != Some(32) {
        return Err(PlatformError::UnsupportedVisual(screen.root_depth));
    }
    let lsb_first = conn.setup().image_byte_order == ImageOrder::LSB_FIRST;

    let device_width = (options.width * factor).max(1);
    let device_height = (options.height * factor).max(1);
    let (device_x, device_y) = options
        .position
        .map(|(x, y)| (x * factor as i32, y * factor as i32))
        .unwrap_or((0, 0));

    let window = conn.generate_id()?;
    conn.create_window(
        screen.root_depth,
        window,
        screen.root,
        device_x as i16,
        device_y as i16,
        device_width as u16,
        device_height as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new()
            .background_pixel(screen.white_pixel)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::STRUCTURE_NOTIFY
                    | EventMask::KEY_PRESS
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::LEAVE_WINDOW
                    | EventMask::FOCUS_CHANGE,
            ),
    )?;

    set_title(&conn, window, &atoms, &options.title)?;
    let class = format!("{0}\0{0}\0", options.app_id);
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        class.as_bytes(),
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atoms.WM_PROTOCOLS,
        AtomEnum::ATOM,
        &[atoms.WM_DELETE_WINDOW],
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atoms._NET_WM_PID,
        AtomEnum::CARDINAL,
        &[std::process::id()],
    )?;
    let window_type = match options.kind {
        WindowKind::Normal => atoms._NET_WM_WINDOW_TYPE_NORMAL,
        WindowKind::Dock { .. } => atoms._NET_WM_WINDOW_TYPE_DOCK,
    };
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atoms._NET_WM_WINDOW_TYPE,
        AtomEnum::ATOM,
        &[window_type],
    )?;
    if let WindowKind::Dock { edge, strut } = options.kind {
        set_strut(
            &conn,
            window,
            &atoms,
            edge,
            strut * factor,
            device_x,
            device_width,
        )?;
    }
    if !options.resizable {
        let position = options.position.map(|_| (device_x, device_y));
        set_fixed_size(&conn, window, device_width, device_height, position)?;
    }

    let gc = conn.generate_id()?;
    conn.create_gc(gc, window, &CreateGCAux::new())?;

    let min_keycode = conn.setup().min_keycode;
    let max_keycode = conn.setup().max_keycode;
    let mapping = conn
        .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?
        .reply()?;
    let per_keycode = usize::from(mapping.keysyms_per_keycode.max(1));
    let modifier_mapping = conn.get_modifier_mapping()?.reply()?;
    let num_lock = num_lock_mask(
        &mapping.keysyms,
        per_keycode,
        min_keycode,
        &modifier_mapping,
    );

    conn.map_window(window)?;
    if let Some((x, y)) = options.position {
        conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(x * factor as i32)
                .y(y * factor as i32),
        )?;
    }
    conn.flush()?;

    let wake = WakePipe::new()?;
    on_proxy(wake.proxy.clone());
    let (a11y_sender, a11y_messages) = mpsc::channel();
    let mut accessibility = Accessibility::new(a11y_sender, wake.proxy.clone());

    let mut logical = (options.width, options.height);
    let mut surface = RasterSurface::new(logical.0, logical.1, scale)?;
    let mut needs_paint = true;
    let mut timers: Vec<(Instant, u64)> = Vec::new();
    let max_request = conn.maximum_request_bytes();
    let mut frame = Vec::new();

    let mut control = Control {
        scale: Some(scale),
        ..Control::default()
    };
    app.event(
        &Event::Resized {
            width: logical.0,
            height: logical.1,
        },
        &mut control,
    );

    loop {
        // Apply what the app asked for during the last batch of events.
        if control.quit {
            break;
        }
        if let Some(title) = control.title.take() {
            set_title(&conn, window, &atoms, &title)?;
        }
        if let Some(rect) = control.geometry.take() {
            let rect: crate::foundation::geometry::Rect = rect;
            if !options.resizable {
                // The WM clamps to the old fixed size unless the hints move too.
                set_fixed_size(
                    &conn,
                    window,
                    rect.width * factor,
                    rect.height * factor,
                    Some((rect.x * factor as i32, rect.y * factor as i32)),
                )?;
            }
            conn.configure_window(
                window,
                &ConfigureWindowAux::new()
                    .x(rect.x * factor as i32)
                    .y(rect.y * factor as i32)
                    .width(rect.width * factor)
                    .height(rect.height * factor),
            )?;
            if let WindowKind::Dock { edge, strut } = options.kind {
                set_strut(
                    &conn,
                    window,
                    &atoms,
                    edge,
                    strut * factor,
                    rect.x * factor as i32,
                    rect.width * factor,
                )?;
            }
        }
        let now = Instant::now();
        timers.extend(
            control
                .timers
                .drain(..)
                .map(|(delay, id)| (now + delay, id)),
        );
        if control.redraw {
            needs_paint = true;
        }
        if control.accessibility_changed {
            accessibility.update(&app, &options.title);
        }
        control.redraw = false;
        control.accessibility_changed = false;

        if needs_paint {
            needs_paint = false;
            app.paint(&surface);
            present(
                &conn,
                window,
                gc,
                &surface,
                screen.root_depth,
                lsb_first,
                max_request,
                &mut frame,
            )?;
        }
        conn.flush()?;

        // Handle everything already queued before blocking.
        let mut handled = false;
        while let Some(event) = conn.poll_for_event()? {
            handled = true;
            let translated = translate(
                &event,
                factor,
                &mapping.keysyms,
                per_keycode,
                min_keycode,
                num_lock,
                &atoms,
            );
            match event {
                XEvent::Expose(expose) if expose.count == 0 => needs_paint = true,
                XEvent::ConfigureNotify(configure) if configure.window == window => {
                    let size = (
                        (u32::from(configure.width) / factor).max(1),
                        (u32::from(configure.height) / factor).max(1),
                    );
                    if size != logical {
                        logical = size;
                        surface = RasterSurface::new(size.0, size.1, scale)?;
                        needs_paint = true;
                    }
                    accessibility.set_bounds(&conn, window, screen.root);
                }
                XEvent::FocusIn(_) => accessibility.focus(true),
                XEvent::FocusOut(_) => accessibility.focus(false),
                // Destroyed from outside (xkill, a crashed WM's cleanup): there
                // is nothing left to show, so give the app its close and stop.
                XEvent::DestroyNotify(destroy) if destroy.window == window => {
                    app.event(&Event::CloseRequested, &mut control);
                    return Ok(());
                }
                _ => {}
            }
            if let Some(event) = translated {
                app.event(&event, &mut control);
            }
        }
        if handled {
            continue;
        }

        let timeout = timers
            .iter()
            .map(|(deadline, _)| deadline.saturating_duration_since(Instant::now()))
            .min();
        let signalled = wait(conn.stream().as_raw_fd(), wake.read.as_raw_fd(), timeout)?;

        let now = Instant::now();
        let (due, pending): (Vec<_>, Vec<_>) =
            timers.drain(..).partition(|(deadline, _)| *deadline <= now);
        timers = pending;
        for (_, id) in due {
            app.event(&Event::Timer(id), &mut control);
        }
        let drained = wake.drain();
        let mut woke = false;
        for message in a11y_messages.try_iter() {
            match message {
                A11yMessage::Activated => accessibility.update(&app, &options.title),
                A11yMessage::Request(request) => {
                    app.event(&Event::Accessibility(request), &mut control)
                }
            }
            woke = true;
        }
        if (signalled || drained) && !woke {
            app.event(&Event::Wake, &mut control);
        }
    }

    conn.destroy_window(window)?;
    conn.flush()?;
    Ok(())
}

fn set_title(
    conn: &RustConnection,
    window: u32,
    atoms: &Atoms,
    title: &str,
) -> Result<(), PlatformError> {
    conn.change_property8(
        PropMode::REPLACE,
        window,
        atoms._NET_WM_NAME,
        atoms.UTF8_STRING,
        title.as_bytes(),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        title.as_bytes(),
    )?;
    Ok(())
}

/// Pin a non-resizable window's size (and, when given, its position) in
/// `WM_NORMAL_HINTS`.
fn set_fixed_size(
    conn: &RustConnection,
    window: u32,
    width: u32,
    height: u32,
    position: Option<(i32, i32)>,
) -> Result<(), PlatformError> {
    let mut hints = WmSizeHints::new();
    hints.min_size = Some((width as i32, height as i32));
    hints.max_size = Some((width as i32, height as i32));
    hints.position = position.map(|(x, y)| {
        (
            x11rb::properties::WmSizeHintsSpecification::ProgramSpecified,
            x,
            y,
        )
    });
    hints.set_normal_hints(conn, window)?;
    Ok(())
}

fn set_strut(
    conn: &RustConnection,
    window: u32,
    atoms: &Atoms,
    edge: Edge,
    size: u32,
    x: i32,
    width: u32,
) -> Result<(), PlatformError> {
    let start = x.max(0) as u32;
    let end = start + width.saturating_sub(1);
    // left, right, top, bottom, then start/end pairs for each edge.
    let partial = match edge {
        Edge::Top => [0, 0, size, 0, 0, 0, 0, 0, start, end, 0, 0],
        Edge::Bottom => [0, 0, 0, size, 0, 0, 0, 0, 0, 0, start, end],
    };
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atoms._NET_WM_STRUT_PARTIAL,
        AtomEnum::CARDINAL,
        &partial,
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atoms._NET_WM_STRUT,
        AtomEnum::CARDINAL,
        &partial[..4],
    )?;
    Ok(())
}

/// The modifier bit the server maps the Num Lock key to. Usually Mod2
/// (`0x0010`), but the mapping is server-specific.
fn num_lock_mask(
    keysyms: &[u32],
    per_keycode: usize,
    min_keycode: u8,
    mapping: &GetModifierMappingReply,
) -> u16 {
    const NUM_LOCK: u32 = 0xff7f;
    let Some(keycode) = keysyms
        .chunks(per_keycode)
        .position(|row| row.contains(&NUM_LOCK))
        .and_then(|index| u8::try_from(index).ok())
        .map(|index| index.saturating_add(min_keycode))
    else {
        return 0;
    };
    let per_modifier = usize::from(mapping.keycodes_per_modifier().max(1));
    mapping
        .keycodes
        .iter()
        .position(|mapped| *mapped == keycode)
        .map(|index| index / per_modifier)
        .filter(|modifier| *modifier < 8)
        .map_or(0, |modifier| 1u16 << modifier)
}

/// Block until the X connection or the wake pipe is readable, or `timeout`.
/// Reports whether the wake pipe was readable.
fn wait(x_fd: RawFd, wake_fd: RawFd, timeout: Option<Duration>) -> std::io::Result<bool> {
    let mut fds = [
        libc::pollfd {
            fd: x_fd,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: wake_fd,
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    let timeout_ms = timeout
        .map(|duration| duration.as_millis().min(i32::MAX as u128) as i32)
        .unwrap_or(-1);
    // SAFETY: polling two valid descriptors in a local array.
    let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
    if result < 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    Ok(fds[1].revents & libc::POLLIN != 0)
}

#[allow(clippy::too_many_arguments)]
fn present(
    conn: &RustConnection,
    window: u32,
    gc: u32,
    surface: &RasterSurface,
    depth: u8,
    lsb_first: bool,
    max_request: usize,
    frame: &mut Vec<u8>,
) -> Result<(), PlatformError> {
    let width = surface.device_width();
    let height = surface.device_height();
    frame.clear();
    frame.reserve(width as usize * height as usize * 4);
    for [red, green, blue, _] in surface.data().as_chunks::<4>().0 {
        // Premultiplied RGBA over black is exactly what an opaque window shows.
        if lsb_first {
            frame.extend_from_slice(&[*blue, *green, *red, 0]);
        } else {
            frame.extend_from_slice(&[0, *red, *green, *blue]);
        }
    }
    let row_bytes = width as usize * 4;
    let rows_per_request = ((max_request.saturating_sub(64)) / row_bytes).max(1);
    for (index, chunk) in frame.chunks(rows_per_request * row_bytes).enumerate() {
        let rows = chunk.len() / row_bytes;
        conn.put_image(
            ImageFormat::Z_PIXMAP,
            window,
            gc,
            width as u16,
            rows as u16,
            0,
            (index * rows_per_request) as i16,
            0,
            depth,
            chunk,
        )?;
    }
    Ok(())
}

fn translate(
    event: &XEvent,
    factor: u32,
    keysyms: &[u32],
    per_keycode: usize,
    min_keycode: u8,
    num_lock: u16,
    atoms: &Atoms,
) -> Option<Event> {
    let scale = |value: i16| i32::from(value) / factor as i32;
    let button = |detail: u8| match detail {
        1 => PointerButton::Primary,
        2 => PointerButton::Middle,
        3 => PointerButton::Secondary,
        other => PointerButton::Other(other),
    };
    match event {
        XEvent::MotionNotify(motion) => Some(Event::PointerMoved {
            x: scale(motion.event_x),
            y: scale(motion.event_y),
        }),
        XEvent::ButtonPress(press) => match press.detail {
            4 => Some(Event::Scroll { dx: 0, dy: -1 }),
            5 => Some(Event::Scroll { dx: 0, dy: 1 }),
            6 => Some(Event::Scroll { dx: -1, dy: 0 }),
            7 => Some(Event::Scroll { dx: 1, dy: 0 }),
            detail => Some(Event::PointerPressed {
                x: scale(press.event_x),
                y: scale(press.event_y),
                button: button(detail),
            }),
        },
        XEvent::ButtonRelease(release) if !(4..=7).contains(&release.detail) => {
            Some(Event::PointerReleased {
                x: scale(release.event_x),
                y: scale(release.event_y),
                button: button(release.detail),
            })
        }
        XEvent::LeaveNotify(_) => Some(Event::PointerLeft),
        XEvent::KeyPress(press) => {
            let index = usize::from(press.detail.saturating_sub(min_keycode)) * per_keycode;
            let row = keysyms.get(index..index + per_keycode).unwrap_or(&[]);
            let state = u16::from(press.state);
            Some(Event::KeyPressed {
                key: key_for(keysym_for(row, state, num_lock)),
                modifiers: modifiers(state),
            })
        }
        XEvent::FocusIn(_) => Some(Event::FocusChanged(true)),
        XEvent::FocusOut(_) => Some(Event::FocusChanged(false)),
        XEvent::ConfigureNotify(configure) => Some(Event::Resized {
            width: (u32::from(configure.width) / factor).max(1),
            height: (u32::from(configure.height) / factor).max(1),
        }),
        XEvent::ClientMessage(message) if is_delete(message, atoms) => Some(Event::CloseRequested),
        _ => None,
    }
}

fn is_delete(message: &ClientMessageEvent, atoms: &Atoms) -> bool {
    message.type_ == atoms.WM_PROTOCOLS && message.data.as_data32()[0] == atoms.WM_DELETE_WINDOW
}

/// AT-SPI exposure through AccessKit. Without the `atspi` feature this only
/// keeps the bookkeeping, so apps compile the same either way.
struct Accessibility {
    #[cfg(feature = "atspi")]
    adapter: accesskit_unix::Adapter,
}

#[cfg(feature = "atspi")]
mod atspi {
    use super::{A11yMessage, Proxy};
    use crate::event::AccessibilityRequest;
    use std::sync::mpsc::Sender;

    pub struct Activation(pub Sender<A11yMessage>, pub Proxy);
    pub struct Actions(pub Sender<A11yMessage>, pub Proxy);
    pub struct Deactivation;

    impl accesskit::ActivationHandler for Activation {
        fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            // The tree is built on the GUI thread; ask it to send one.
            let _ = self.0.send(A11yMessage::Activated);
            self.1.wake();
            None
        }
    }

    impl accesskit::ActionHandler for Actions {
        fn do_action(&mut self, request: accesskit::ActionRequest) {
            let node = request.target_node.0;
            let request = match request.action {
                accesskit::Action::Click => AccessibilityRequest::Click(node),
                accesskit::Action::Focus => AccessibilityRequest::Focus(node),
                _ => return,
            };
            let _ = self.0.send(A11yMessage::Request(request));
            self.1.wake();
        }
    }

    impl accesskit::DeactivationHandler for Deactivation {
        fn deactivate_accessibility(&mut self) {}
    }
}

impl Accessibility {
    fn new(sender: Sender<A11yMessage>, proxy: Proxy) -> Self {
        #[cfg(feature = "atspi")]
        {
            let adapter = accesskit_unix::Adapter::new(
                atspi::Activation(sender.clone(), proxy.clone()),
                atspi::Actions(sender, proxy),
                atspi::Deactivation,
            );
            Self { adapter }
        }
        #[cfg(not(feature = "atspi"))]
        {
            let _ = (sender, proxy);
            Self {}
        }
    }

    fn update<A: App>(&mut self, app: &A, title: &str) {
        #[cfg(feature = "atspi")]
        self.adapter
            .update_if_active(|| app.accessibility().unwrap_or_else(|| fallback_tree(title)));
        #[cfg(not(feature = "atspi"))]
        let _ = (app, title);
    }

    fn focus(&mut self, focused: bool) {
        #[cfg(feature = "atspi")]
        self.adapter.update_window_focus_state(focused);
        #[cfg(not(feature = "atspi"))]
        let _ = focused;
    }

    fn set_bounds(&mut self, conn: &RustConnection, window: u32, root: u32) {
        #[cfg(feature = "atspi")]
        if let (Ok(cookie), Ok(geometry)) = (
            conn.translate_coordinates(window, root, 0, 0),
            conn.get_geometry(window),
        ) {
            if let (Ok(origin), Ok(geometry)) = (cookie.reply(), geometry.reply()) {
                let rect = accesskit::Rect {
                    x0: f64::from(origin.dst_x),
                    y0: f64::from(origin.dst_y),
                    x1: f64::from(origin.dst_x) + f64::from(geometry.width),
                    y1: f64::from(origin.dst_y) + f64::from(geometry.height),
                };
                self.adapter.set_root_window_bounds(rect, rect);
            }
        }
        #[cfg(not(feature = "atspi"))]
        let _ = (conn, window, root);
    }
}

#[cfg(feature = "atspi")]
fn fallback_tree(title: &str) -> accesskit::TreeUpdate {
    let root = accesskit::NodeId(0);
    let mut node = accesskit::Node::new(accesskit::Role::Window);
    node.set_label(title);
    accesskit::TreeUpdate {
        nodes: vec![(root, node)],
        tree: Some(accesskit::TreeInfo::new(root)),
        tree_id: accesskit::TreeId::ROOT,
        focus: root,
    }
}

/// Rect helper for apps that position docks against the screen.
pub fn screen_size() -> Result<(u32, u32), PlatformError> {
    let (conn, screen_number) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_number];
    Ok((
        u32::from(screen.width_in_pixels),
        u32::from(screen.height_in_pixels),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modifier_mapping(keycodes: &[u8]) -> GetModifierMappingReply {
        let mut keycodes = keycodes.to_vec();
        let per_modifier = (keycodes.len() / 8).max(1);
        keycodes.resize(per_modifier * 8, 0);
        GetModifierMappingReply {
            sequence: 0,
            length: 0,
            keycodes,
        }
    }

    #[test]
    fn num_lock_bit_comes_from_the_modifier_mapping() {
        // Keycode 9 is the second key in the mapping row: its first keysym
        // pair is an ordinary key, and the second pair is Num_Lock.
        let row = [0x0061, 0x0061, 0xff7f, 0xff7f];
        // Num_Lock is keycode 9 when the minimum keycode is 8, and here the
        // server maps it to the second modifier group (LockMask).
        let mapping = modifier_mapping(&[0, 9, 0, 0, 0, 0, 0, 0]);
        assert_eq!(num_lock_mask(&row, 2, 8, &mapping), 0x0002);
        let unmapped = modifier_mapping(&[0; 8]);
        assert_eq!(num_lock_mask(&row, 2, 8, &unmapped), 0);
        assert_eq!(num_lock_mask(&[0x0061], 1, 8, &mapping), 0);
    }

    #[test]
    fn wake_pipe_reports_signals_until_drained() {
        let pipe = WakePipe::new().unwrap();
        assert!(!pipe.drain());
        pipe.proxy.wake();
        assert!(pipe.drain());
        assert!(!pipe.drain());
    }
}
