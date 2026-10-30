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
    EventMask, GetModifierMappingReply, ImageFormat, ImageOrder, PropMode, SelectionNotifyEvent,
    SelectionRequestEvent, WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event as XEvent;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use super::global_menu::Message as MenuMessage;
use super::keys::{grab_masks, key_for, keycodes_for, keysym_for, modifier_mask, modifiers};
use super::{
    place_popup, scale_from_environment, App, Control, Edge, PopupRequest, WindowKind,
    WindowOptions,
};
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
        _NET_WM_WINDOW_TYPE_DESKTOP,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_POPUP_MENU,
        _NET_WM_WINDOW_TYPE_NOTIFICATION,
        _NET_WM_STRUT,
        _NET_WM_STRUT_PARTIAL,
        CLIPBOARD,
        TARGETS,
        SLOPOS_PASTE,
        _GTK_UNIQUE_BUS_NAME,
        _GTK_MENUBAR_OBJECT_PATH,
        _GTK_APPLICATION_OBJECT_PATH,
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

/// A wake proxy whose pipe nobody reads (tests of code that wakes an app).
#[cfg(test)]
pub(crate) fn test_proxy() -> Proxy {
    let pipe = WakePipe::new().expect("pipe");
    let proxy = pipe.proxy.clone();
    std::mem::forget(pipe.read);
    proxy
}

/// Messages from the AccessKit adapter thread.
#[cfg_attr(not(feature = "atspi"), allow(dead_code))]
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
        WindowKind::Desktop => atoms._NET_WM_WINDOW_TYPE_DESKTOP,
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

    // The wake pipe exists before the window maps so the global-menu export
    // can be advertised on the window before the shell first sees it.
    let wake = WakePipe::new()?;
    let (menu_sender, menu_messages) = mpsc::channel();
    let menus = GlobalMenu::start(
        &conn,
        window,
        &atoms,
        &options.app_id,
        &app,
        menu_sender,
        wake.proxy.clone(),
    )?;

    let min_keycode = conn.setup().min_keycode;
    let max_keycode = conn.setup().max_keycode;
    let mut keymap = Keymap::read(&conn, min_keycode, max_keycode)?;
    grab_global_keys(
        &conn,
        screen.root,
        &keymap,
        min_keycode,
        &options.global_keys,
    )?;

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

    on_proxy(wake.proxy.clone());
    let (a11y_sender, a11y_messages) = mpsc::channel();
    let mut accessibility = Accessibility::new(a11y_sender, wake.proxy.clone());

    let mut logical = (options.width, options.height);
    let mut surface = RasterSurface::new(logical.0, logical.1, scale)?;
    let mut needs_paint = true;
    let mut timers: Vec<(Instant, u64)> = Vec::new();
    let mut clicks = ClickTracker::default();
    // Text this window offers on CLIPBOARD while it owns the selection.
    let mut clipboard: Option<String> = None;
    let max_request = conn.maximum_request_bytes();
    let mut frame = Vec::new();
    let mut popups: Vec<Popup> = Vec::new();

    let mut control = Control {
        scale: Some(scale),
        ..Control::default()
    };
    if menus.shell_present() {
        app.event(&Event::GlobalMenu(true), &mut control);
    }
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
            for popup in &mut popups {
                popup.needs_paint = true;
            }
        }
        for request in std::mem::take(&mut control.popups) {
            apply_popup(&conn, &screen, window, &atoms, scale, &mut popups, request)?;
        }
        if control.accessibility_changed {
            accessibility.update(&app, &options.title);
            // Menu state follows the same changes as the accessibility tree.
            menus.update(&app);
        }
        control.redraw = false;
        control.accessibility_changed = false;
        if let Some(text) = control.clipboard.take() {
            conn.set_selection_owner(window, atoms.CLIPBOARD, x11rb::CURRENT_TIME)?;
            clipboard = Some(text);
        }
        if std::mem::take(&mut control.paste) {
            match &clipboard {
                // We own the selection: no round trip through the server.
                Some(text) => {
                    let text = text.clone();
                    app.event(&Event::Paste(text), &mut control);
                    continue;
                }
                None => {
                    conn.convert_selection(
                        window,
                        atoms.CLIPBOARD,
                        atoms.UTF8_STRING,
                        atoms.SLOPOS_PASTE,
                        x11rb::CURRENT_TIME,
                    )?;
                }
            }
        }

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
        for popup in &mut popups {
            if std::mem::take(&mut popup.needs_paint) {
                app.paint_popup(popup.id, &popup.surface);
                present(
                    &conn,
                    popup.window,
                    gc,
                    &popup.surface,
                    screen.root_depth,
                    lsb_first,
                    max_request,
                    &mut frame,
                )?;
            }
        }
        conn.flush()?;

        // Handle everything already queued before blocking.
        let mut handled = false;
        while let Some(event) = conn.poll_for_event()? {
            handled = true;
            if let XEvent::MappingNotify(_) = event {
                // Layout switches and tools like xdotool remap keycodes at
                // run time; stale keysyms would drop or garble typed text.
                keymap = Keymap::read(&conn, min_keycode, max_keycode)?;
                conn.ungrab_key(
                    x11rb::protocol::xproto::Grab::ANY,
                    screen.root,
                    x11rb::protocol::xproto::ModMask::ANY,
                )?;
                grab_global_keys(
                    &conn,
                    screen.root,
                    &keymap,
                    min_keycode,
                    &options.global_keys,
                )?;
                continue;
            }
            // Global shortcuts arrive on the root window we grabbed them on.
            if let XEvent::KeyPress(press) = &event {
                if press.event == screen.root {
                    let row_start =
                        usize::from(press.detail.saturating_sub(min_keycode)) * keymap.per_keycode;
                    let row = keymap
                        .keysyms
                        .get(row_start..row_start + keymap.per_keycode)
                        .unwrap_or(&[]);
                    let state = u16::from(press.state);
                    let pressed = modifiers(state);
                    if let Some((keysym, _)) = options
                        .global_keys
                        .iter()
                        .find(|(keysym, wanted)| *wanted == pressed && row.contains(keysym))
                    {
                        app.event(
                            &Event::GlobalKey {
                                keysym: *keysym,
                                modifiers: pressed,
                            },
                            &mut control,
                        );
                    }
                    continue;
                }
            }
            let state = match &event {
                XEvent::ButtonPress(e) => Some(u16::from(e.state)),
                XEvent::ButtonRelease(e) => Some(u16::from(e.state)),
                XEvent::MotionNotify(e) => Some(u16::from(e.state)),
                XEvent::KeyPress(e) => Some(u16::from(e.state)),
                _ => None,
            };
            if let Some(state) = state {
                control.modifiers = modifiers(state);
            }
            let primary_press = match &event {
                XEvent::ButtonPress(press) if press.detail == 1 => Some((
                    press.time,
                    i32::from(press.event_x) / factor as i32,
                    i32::from(press.event_y) / factor as i32,
                )),
                _ => None,
            };
            let translated = translate(
                &event,
                factor,
                &keymap.keysyms,
                keymap.per_keycode,
                min_keycode,
                keymap.num_lock,
                &atoms,
            );
            // Popup windows get their exposes and input; nothing else.
            if let Some(popup) = event_window(&event)
                .and_then(|target| popups.iter_mut().find(|popup| popup.window == target))
            {
                match (&event, translated) {
                    (XEvent::Expose(expose), _) if expose.count == 0 => popup.needs_paint = true,
                    (
                        XEvent::ButtonPress(_)
                        | XEvent::ButtonRelease(_)
                        | XEvent::MotionNotify(_)
                        | XEvent::KeyPress(_)
                        | XEvent::LeaveNotify(_),
                        Some(input),
                    ) => {
                        let id = popup.id;
                        app.event(
                            &Event::Popup {
                                id,
                                event: Box::new(input),
                            },
                            &mut control,
                        );
                    }
                    _ => {}
                }
                continue;
            }
            match event {
                XEvent::Expose(expose) if expose.count == 0 && expose.window == window => {
                    needs_paint = true
                }
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
                XEvent::SelectionRequest(request) => {
                    answer_selection(&conn, &atoms, &request, clipboard.as_deref())?;
                }
                XEvent::SelectionClear(clear) if clear.selection == atoms.CLIPBOARD => {
                    clipboard = None;
                }
                XEvent::SelectionNotify(notify)
                    if notify.requestor == window && notify.property == atoms.SLOPOS_PASTE =>
                {
                    let reply = conn
                        .get_property(
                            true,
                            window,
                            atoms.SLOPOS_PASTE,
                            AtomEnum::ANY,
                            0,
                            (super::MAX_CLIPBOARD_BYTES / 4) as u32,
                        )?
                        .reply()?;
                    if reply.format == 8 && !reply.value.is_empty() {
                        let text = String::from_utf8_lossy(&reply.value).into_owned();
                        app.event(&Event::Paste(text), &mut control);
                    }
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
                if let Some((time, x, y)) = primary_press {
                    if clicks.press(time, x, y) {
                        app.event(&Event::DoubleClicked { x, y }, &mut control);
                    }
                }
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
        for message in menu_messages.try_iter() {
            let event = match message {
                MenuMessage::Activate { menu, item } => Event::MenuChosen { menu, item },
                MenuMessage::ShellPresent(present) => Event::GlobalMenu(present),
            };
            app.event(&event, &mut control);
            control.accessibility_changed = true;
        }
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

    for popup in popups.drain(..) {
        conn.destroy_window(popup.window)?;
    }
    conn.destroy_window(window)?;
    conn.flush()?;
    Ok(())
}

/// Grab each global shortcut on the root window under every lock state.
fn grab_global_keys(
    conn: &RustConnection,
    root: u32,
    keymap: &Keymap,
    min_keycode: u8,
    keys: &[(u32, crate::event::Modifiers)],
) -> Result<(), PlatformError> {
    for (keysym, wanted) in keys {
        let base = modifier_mask(*wanted);
        for keycode in keycodes_for(*keysym, &keymap.keysyms, keymap.per_keycode, min_keycode) {
            for mask in grab_masks(base, keymap.num_lock) {
                let grabbed = conn
                    .grab_key(
                        true,
                        root,
                        x11rb::protocol::xproto::ModMask::from(mask),
                        keycode,
                        x11rb::protocol::xproto::GrabMode::ASYNC,
                        x11rb::protocol::xproto::GrabMode::ASYNC,
                    )?
                    .check();
                if let Err(error) = grabbed {
                    log::warn!("global shortcut {keysym:#x} (mask {base:#x}) is taken: {error}");
                }
            }
        }
    }
    Ok(())
}

/// A popup window (menu, popover or toast) owned by the app window.
struct Popup {
    id: u64,
    window: u32,
    grab: bool,
    surface: RasterSurface,
    needs_paint: bool,
}

/// The window an input or expose event is addressed to.
fn event_window(event: &XEvent) -> Option<u32> {
    Some(match event {
        XEvent::Expose(e) => e.window,
        XEvent::ButtonPress(e) => e.event,
        XEvent::ButtonRelease(e) => e.event,
        XEvent::MotionNotify(e) => e.event,
        XEvent::KeyPress(e) => e.event,
        XEvent::LeaveNotify(e) => e.event,
        XEvent::ConfigureNotify(e) => e.window,
        _ => return None,
    })
}

/// Open, move or close a popup window as the app asked.
fn apply_popup(
    conn: &RustConnection,
    screen: &x11rb::protocol::xproto::Screen,
    parent: u32,
    atoms: &Atoms,
    scale: crate::foundation::geometry::Scale,
    popups: &mut Vec<Popup>,
    request: PopupRequest,
) -> Result<(), PlatformError> {
    let factor = scale.factor();
    match request {
        PopupRequest::Close { id } => {
            if let Some(index) = popups.iter().position(|popup| popup.id == id) {
                let popup = popups.remove(index);
                conn.destroy_window(popup.window)?;
                if popup.grab && !popups.iter().any(|other| other.grab) {
                    conn.ungrab_pointer(x11rb::CURRENT_TIME)?;
                    conn.ungrab_keyboard(x11rb::CURRENT_TIME)?;
                }
            }
        }
        PopupRequest::Open { id, rect, grab } => {
            let origin = conn
                .translate_coordinates(parent, screen.root, 0, 0)?
                .reply()
                .map(|reply| (i32::from(reply.dst_x), i32::from(reply.dst_y)))
                .unwrap_or((0, 0));
            let screen_size = (
                u32::from(screen.width_in_pixels),
                u32::from(screen.height_in_pixels),
            );
            let placed = place_popup(origin, rect, factor, screen_size);
            let logical = (
                (placed.width / factor).max(1),
                (placed.height / factor).max(1),
            );
            if let Some(popup) = popups.iter_mut().find(|popup| popup.id == id) {
                conn.configure_window(
                    popup.window,
                    &ConfigureWindowAux::new()
                        .x(placed.x)
                        .y(placed.y)
                        .width(placed.width)
                        .height(placed.height)
                        .stack_mode(x11rb::protocol::xproto::StackMode::ABOVE),
                )?;
                if (
                    popup.surface.logical_width(),
                    popup.surface.logical_height(),
                ) != logical
                {
                    popup.surface = RasterSurface::new(logical.0, logical.1, scale)?;
                }
                popup.needs_paint = true;
                return Ok(());
            }
            let popup_window = conn.generate_id()?;
            conn.create_window(
                screen.root_depth,
                popup_window,
                screen.root,
                placed.x as i16,
                placed.y as i16,
                placed.width as u16,
                placed.height as u16,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new()
                    .background_pixel(screen.white_pixel)
                    .override_redirect(1)
                    .save_under(1)
                    .event_mask(
                        EventMask::EXPOSURE
                            | EventMask::KEY_PRESS
                            | EventMask::BUTTON_PRESS
                            | EventMask::BUTTON_RELEASE
                            | EventMask::POINTER_MOTION
                            | EventMask::LEAVE_WINDOW,
                    ),
            )?;
            let kind = if grab {
                atoms._NET_WM_WINDOW_TYPE_POPUP_MENU
            } else {
                atoms._NET_WM_WINDOW_TYPE_NOTIFICATION
            };
            conn.change_property32(
                PropMode::REPLACE,
                popup_window,
                atoms._NET_WM_WINDOW_TYPE,
                AtomEnum::ATOM,
                &[kind],
            )?;
            conn.change_property32(
                PropMode::REPLACE,
                popup_window,
                AtomEnum::WM_TRANSIENT_FOR,
                AtomEnum::WINDOW,
                &[parent],
            )?;
            conn.map_window(popup_window)?;
            if grab {
                // Owner events: presses on the app's own windows reach them
                // as usual; presses anywhere else come to the popup with
                // coordinates outside it, so a menu can dismiss itself.
                conn.grab_pointer(
                    true,
                    popup_window,
                    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                    x11rb::protocol::xproto::GrabMode::ASYNC,
                    x11rb::protocol::xproto::GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    x11rb::CURRENT_TIME,
                )?;
                conn.grab_keyboard(
                    true,
                    popup_window,
                    x11rb::CURRENT_TIME,
                    x11rb::protocol::xproto::GrabMode::ASYNC,
                    x11rb::protocol::xproto::GrabMode::ASYNC,
                )?;
            }
            popups.push(Popup {
                id,
                window: popup_window,
                grab,
                surface: RasterSurface::new(logical.0, logical.1, scale)?,
                needs_paint: true,
            });
        }
    }
    Ok(())
}

/// The window's export to the desktop's global menu bar, when the app has
/// menus, the `global-menu` feature is on and a session bus is reachable.
struct GlobalMenu {
    #[cfg(feature = "global-menu")]
    exporter: Option<super::global_menu::Exporter>,
}

impl GlobalMenu {
    #[allow(unused_variables)]
    fn start<A: App>(
        conn: &RustConnection,
        window: u32,
        atoms: &Atoms,
        app_id: &str,
        app: &A,
        sender: Sender<MenuMessage>,
        proxy: Proxy,
    ) -> Result<Self, PlatformError> {
        #[cfg(feature = "global-menu")]
        {
            let exporter = app.menu_bar().and_then(|bar| {
                super::global_menu::Exporter::start(
                    app_id,
                    super::global_menu::Snapshot::of(bar),
                    sender,
                    proxy,
                )
            });
            if let Some(exporter) = &exporter {
                if let Some(name) = exporter.unique_name() {
                    for (atom, value) in [
                        (atoms._GTK_UNIQUE_BUS_NAME, name.as_str()),
                        (atoms._GTK_MENUBAR_OBJECT_PATH, exporter.menu_path()),
                        (atoms._GTK_APPLICATION_OBJECT_PATH, exporter.action_path()),
                    ] {
                        conn.change_property8(
                            PropMode::REPLACE,
                            window,
                            atom,
                            atoms.UTF8_STRING,
                            value.as_bytes(),
                        )?;
                    }
                }
            }
            Ok(Self { exporter })
        }
        #[cfg(not(feature = "global-menu"))]
        Ok(Self {})
    }

    fn shell_present(&self) -> bool {
        #[cfg(feature = "global-menu")]
        return self
            .exporter
            .as_ref()
            .is_some_and(|exporter| exporter.shell_present());
        #[cfg(not(feature = "global-menu"))]
        false
    }

    #[allow(unused_variables)]
    fn update<A: App>(&self, app: &A) {
        #[cfg(feature = "global-menu")]
        if let (Some(exporter), Some(bar)) = (&self.exporter, app.menu_bar()) {
            exporter.update(super::global_menu::Snapshot::of(bar));
        }
    }
}

/// Answer another client's request for our CLIPBOARD contents (ICCCM §2.2).
fn answer_selection(
    conn: &RustConnection,
    atoms: &Atoms,
    request: &SelectionRequestEvent,
    text: Option<&str>,
) -> Result<(), PlatformError> {
    // Obsolete clients send property None; ICCCM says use the target.
    let property = if request.property == x11rb::NONE {
        request.target
    } else {
        request.property
    };
    let answered = match text {
        Some(_) if request.selection != atoms.CLIPBOARD => false,
        Some(_) if request.target == atoms.TARGETS => {
            let targets = [
                atoms.TARGETS,
                atoms.UTF8_STRING,
                u32::from(AtomEnum::STRING),
            ];
            conn.change_property32(
                PropMode::REPLACE,
                request.requestor,
                property,
                AtomEnum::ATOM,
                &targets,
            )?;
            true
        }
        Some(text)
            if request.target == atoms.UTF8_STRING
                || request.target == u32::from(AtomEnum::STRING) =>
        {
            conn.change_property8(
                PropMode::REPLACE,
                request.requestor,
                property,
                request.target,
                text.as_bytes(),
            )?;
            true
        }
        _ => false,
    };
    let notify = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        time: request.time,
        requestor: request.requestor,
        selection: request.selection,
        target: request.target,
        property: if answered { property } else { x11rb::NONE },
    };
    conn.send_event(false, request.requestor, EventMask::NO_EVENT, notify)?;
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
/// Double-click rule: a second primary press within 400 ms and 4 logical
/// pixels of the first. A third press starts a new pair.
#[derive(Debug, Default)]
struct ClickTracker {
    last: Option<(u32, i32, i32)>,
}

impl ClickTracker {
    const INTERVAL_MS: u32 = 400;
    const SLOP: i32 = 4;

    fn press(&mut self, time: u32, x: i32, y: i32) -> bool {
        let double = self.last.is_some_and(|(last, lx, ly)| {
            time.wrapping_sub(last) <= Self::INTERVAL_MS
                && (x - lx).abs() <= Self::SLOP
                && (y - ly).abs() <= Self::SLOP
        });
        self.last = if double { None } else { Some((time, x, y)) };
        double
    }
}

/// The server's keycode → keysym table and the Num Lock modifier bit.
struct Keymap {
    keysyms: Vec<u32>,
    per_keycode: usize,
    num_lock: u16,
}

impl Keymap {
    fn read(
        conn: &RustConnection,
        min_keycode: u8,
        max_keycode: u8,
    ) -> Result<Self, PlatformError> {
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
        Ok(Self {
            keysyms: mapping.keysyms,
            per_keycode,
            num_lock,
        })
    }
}

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
    put_rgba(
        conn,
        window,
        gc,
        (surface.device_width(), surface.device_height()),
        &surface.data(),
        depth,
        lsb_first,
        max_request,
        frame,
    )
}

/// Upload premultiplied RGBA to a 32-bit-per-pixel drawable, splitting the
/// rows across as many `PutImage` requests as the server's limit needs.
#[allow(clippy::too_many_arguments)]
fn put_rgba(
    conn: &RustConnection,
    drawable: u32,
    gc: u32,
    (width, height): (u32, u32),
    data: &[u8],
    depth: u8,
    lsb_first: bool,
    max_request: usize,
    frame: &mut Vec<u8>,
) -> Result<(), PlatformError> {
    frame.clear();
    frame.reserve(width as usize * height as usize * 4);
    for [red, green, blue, _] in data.as_chunks::<4>().0 {
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
            drawable,
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
                accesskit::Action::Increment => AccessibilityRequest::Increment(node),
                accesskit::Action::Decrement => AccessibilityRequest::Decrement(node),
                accesskit::Action::SetValue => match request.data {
                    Some(accesskit::ActionData::Value(value)) => {
                        AccessibilityRequest::SetValue(node, value.into())
                    }
                    _ => return,
                },
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

/// Make `image` the X root window's background, the way `xsetroot`/`feh`
/// do it: a pixmap the server keeps after this connection closes, announced
/// in `_XROOTPMAP_ID`/`ESETROOT_PMAP_ID` so the previous one is freed and
/// pseudo-transparent clients can find it. `compose` receives the screen
/// size and returns the finished background.
pub fn set_root_background(
    compose: impl FnOnce(u32, u32) -> Option<crate::image::Image>,
) -> Result<(), PlatformError> {
    let (conn, screen_number) = x11rb::connect(None)?;
    let screen = conn.setup().roots[screen_number].clone();
    let (width, height) = (
        u32::from(screen.width_in_pixels),
        u32::from(screen.height_in_pixels),
    );
    let bits = conn
        .setup()
        .pixmap_formats
        .iter()
        .find(|format| format.depth == screen.root_depth)
        .map(|format| format.bits_per_pixel);
    if bits != Some(32) {
        return Err(PlatformError::UnsupportedVisual(screen.root_depth));
    }
    let Some(image) =
        compose(width, height).filter(|image| image.width() == width && image.height() == height)
    else {
        return Ok(());
    };
    let xroot = conn.intern_atom(false, b"_XROOTPMAP_ID")?.reply()?.atom;
    let esetroot = conn.intern_atom(false, b"ESETROOT_PMAP_ID")?.reply()?.atom;

    let pixmap = conn.generate_id()?;
    conn.create_pixmap(
        screen.root_depth,
        pixmap,
        screen.root,
        width as u16,
        height as u16,
    )?;
    let gc = conn.generate_id()?;
    conn.create_gc(gc, pixmap, &CreateGCAux::new())?;
    let lsb_first = conn.setup().image_byte_order == ImageOrder::LSB_FIRST;
    put_rgba(
        &conn,
        pixmap,
        gc,
        (width, height),
        image.data(),
        screen.root_depth,
        lsb_first,
        conn.maximum_request_bytes(),
        &mut Vec::new(),
    )?;
    conn.free_gc(gc)?;

    // Free the pixmap an earlier setter left behind (Esetroot convention:
    // only when both properties name it, so we never kill a live client).
    let read = |atom| -> Option<u32> {
        let reply = conn
            .get_property(false, screen.root, atom, AtomEnum::PIXMAP, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let pixmap = reply.value32()?.next();
        pixmap
    };
    if let (Some(old), Some(also)) = (read(xroot), read(esetroot)) {
        if old == also && old != 0 {
            let _ = conn.kill_client(old);
        }
    }

    conn.change_window_attributes(
        screen.root,
        &x11rb::protocol::xproto::ChangeWindowAttributesAux::new().background_pixmap(pixmap),
    )?;
    conn.clear_area(false, screen.root, 0, 0, 0, 0)?;
    for atom in [xroot, esetroot] {
        conn.change_property32(
            PropMode::REPLACE,
            screen.root,
            atom,
            AtomEnum::PIXMAP,
            &[pixmap],
        )?;
    }
    conn.set_close_down_mode(x11rb::protocol::xproto::CloseDown::RETAIN_PERMANENT)?;
    conn.sync()?;
    Ok(())
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
    fn double_clicks_need_time_and_place_and_do_not_chain() {
        let mut clicks = ClickTracker::default();
        assert!(!clicks.press(1000, 10, 10));
        assert!(clicks.press(1300, 12, 9));
        assert!(!clicks.press(1350, 12, 9));
        assert!(!clicks.press(2000, 12, 9));
        assert!(!clicks.press(2100, 30, 9));
        // Server time wraps around after ~49 days.
        let mut clicks = ClickTracker::default();
        assert!(!clicks.press(u32::MAX - 10, 0, 0));
        assert!(clicks.press(50, 0, 0));
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
