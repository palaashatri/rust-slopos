//! wlr-layer-shell exclusive chrome driver for SLOPOS-I (Phase 3).
//!
//! Three surfaces:
//! - **Background** — wallpaper + icons + in-shell windows
//! - **Top** menu bar — `exclusive_zone = menu_h`
//! - **Bottom** dock — `exclusive_zone = dock_h`
//!
//! The Linux session uses this path by default; the session environment also
//! exposes `SLOPOS_LAYER_SHELL_CHROME=1` as an explicit diagnostic marker.

#![cfg(target_os = "linux")]

use anyhow::anyhow;
use slopos_kit::design_tokens::MENU_BAR_HEIGHT_PX;
use slopos_kit::event::{KeyCode, MouseButton};
use slopos_kit::{Event, Widget};
use slopos_sdk::{RawSurfaceRenderer, UiRuntime};
use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use wayland_client::{
    protocol::{wl_keyboard, wl_pointer, wl_registry, wl_surface},
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

use crate::shell_scale::detect_shell_scale_from_env;
use crate::{ShellDesktop, ShellPaintFilter};

const MENU_H: u32 = MENU_BAR_HEIGHT_PX;
const DOCK_H: u32 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChromeSurfaceKind {
    Background,
    Menu,
    MenuPopup,
    SpacesOverview,
    Dock,
}

/// Linux BTN_* codes from `<linux/input-event-codes.h>`.
fn mouse_button_from_linux(code: u32) -> Option<MouseButton> {
    match code {
        0x110 => Some(MouseButton::Left),
        0x111 => Some(MouseButton::Right),
        0x112 => Some(MouseButton::Middle),
        _ => None,
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Map Linux KEY_* (evdev) codes to kit KeyCode.
/// Covers letters/digits/modifiers needed for Super+O/L shell shortcuts and lock typing.
fn keycode_from_linux(code: u32) -> Option<KeyCode> {
    Some(match code {
        1 => KeyCode::Escape,
        2 => KeyCode::Key1,
        3 => KeyCode::Key2,
        4 => KeyCode::Key3,
        5 => KeyCode::Key4,
        6 => KeyCode::Key5,
        7 => KeyCode::Key6,
        8 => KeyCode::Key7,
        9 => KeyCode::Key8,
        10 => KeyCode::Key9,
        11 => KeyCode::Key0,
        12 => KeyCode::Minus,
        13 => KeyCode::Equals,
        14 => KeyCode::Backspace,
        15 => KeyCode::Tab,
        16 => KeyCode::Q,
        17 => KeyCode::W,
        18 => KeyCode::E,
        19 => KeyCode::R,
        20 => KeyCode::T,
        21 => KeyCode::Y,
        22 => KeyCode::U,
        23 => KeyCode::I,
        24 => KeyCode::O,
        25 => KeyCode::P,
        26 => KeyCode::LeftBracket,
        27 => KeyCode::RightBracket,
        28 => KeyCode::Enter,
        29 => KeyCode::ControlLeft,
        30 => KeyCode::A,
        31 => KeyCode::S,
        32 => KeyCode::D,
        33 => KeyCode::F,
        34 => KeyCode::G,
        35 => KeyCode::H,
        36 => KeyCode::J,
        37 => KeyCode::K,
        38 => KeyCode::L,
        39 => KeyCode::Semicolon,
        40 => KeyCode::Quote,
        42 => KeyCode::ShiftLeft,
        43 => KeyCode::Backslash,
        44 => KeyCode::Z,
        45 => KeyCode::X,
        46 => KeyCode::C,
        47 => KeyCode::V,
        48 => KeyCode::B,
        49 => KeyCode::N,
        50 => KeyCode::M,
        51 => KeyCode::Comma,
        52 => KeyCode::Period,
        53 => KeyCode::Slash,
        54 => KeyCode::ShiftRight,
        56 => KeyCode::AltLeft,
        57 => KeyCode::Space,
        58 => KeyCode::CapsLock,
        97 => KeyCode::ControlRight,
        100 => KeyCode::AltRight,
        102 => KeyCode::Home,
        103 => KeyCode::ArrowUp,
        104 => KeyCode::PageUp,
        105 => KeyCode::ArrowLeft,
        106 => KeyCode::ArrowRight,
        107 => KeyCode::End,
        108 => KeyCode::ArrowDown,
        109 => KeyCode::PageDown,
        111 => KeyCode::Delete,
        125 => KeyCode::MetaLeft,
        126 => KeyCode::MetaRight,
        _ => return None,
    })
}

/// Convert a `wl_keyboard.key` value to the Linux evdev code used by the map
/// above.  The Wayland wire value is the XKB keycode with the conventional
/// eight-code offset removed (Smithay serializes it as `raw_code - 8`).
fn keycode_from_wayland(code: u32) -> Option<KeyCode> {
    code.checked_add(8).and_then(keycode_from_linux)
}

/// Decode a Wayland key event into the kit key and its press/release state.
/// Keeping this normalization together ensures releases use the same physical
/// key as presses instead of reinterpreting the wire code independently.
fn key_event_info_from_wayland(
    code: u32,
    state: &WEnum<wl_keyboard::KeyState>,
) -> Option<(KeyCode, bool)> {
    Some((
        keycode_from_wayland(code)?,
        matches!(state, WEnum::Value(wl_keyboard::KeyState::Pressed)),
    ))
}

/// Printable char for lock-password / text fields (US QWERTY, no full xkb).
fn char_from_keycode(code: KeyCode, shift: bool) -> Option<char> {
    let ch = match code {
        KeyCode::A => 'a',
        KeyCode::B => 'b',
        KeyCode::C => 'c',
        KeyCode::D => 'd',
        KeyCode::E => 'e',
        KeyCode::F => 'f',
        KeyCode::G => 'g',
        KeyCode::H => 'h',
        KeyCode::I => 'i',
        KeyCode::J => 'j',
        KeyCode::K => 'k',
        KeyCode::L => 'l',
        KeyCode::M => 'm',
        KeyCode::N => 'n',
        KeyCode::O => 'o',
        KeyCode::P => 'p',
        KeyCode::Q => 'q',
        KeyCode::R => 'r',
        KeyCode::S => 's',
        KeyCode::T => 't',
        KeyCode::U => 'u',
        KeyCode::V => 'v',
        KeyCode::W => 'w',
        KeyCode::X => 'x',
        KeyCode::Y => 'y',
        KeyCode::Z => 'z',
        KeyCode::Key0 => '0',
        KeyCode::Key1 => '1',
        KeyCode::Key2 => '2',
        KeyCode::Key3 => '3',
        KeyCode::Key4 => '4',
        KeyCode::Key5 => '5',
        KeyCode::Key6 => '6',
        KeyCode::Key7 => '7',
        KeyCode::Key8 => '8',
        KeyCode::Key9 => '9',
        KeyCode::Space => ' ',
        KeyCode::Minus => '-',
        KeyCode::Equals => '=',
        _ => return None,
    };
    Some(if shift { ch.to_ascii_uppercase() } else { ch })
}

/// Decode wl_keyboard mods_depressed (standard xkb mask) into kit Modifiers.
fn modifiers_from_xkb_mask(depressed: u32) -> slopos_kit::event::Modifiers {
    slopos_kit::event::Modifiers {
        shift: depressed & (1 << 0) != 0,
        control: depressed & (1 << 2) != 0,
        alt: depressed & (1 << 3) != 0,
        // Mod4 — Super/Logo/Meta on typical Linux layouts
        meta: depressed & (1 << 6) != 0,
    }
}

struct LayerSurf {
    kind: ChromeSurfaceKind,
    wl: wl_surface::WlSurface,
    layer: ZwlrLayerSurfaceV1,
    configured: Option<(u32, u32)>,
    renderer: Option<RawSurfaceRenderer>,
}

fn scale_surface_dimension(value: u32, buffer_scale: u32) -> u32 {
    value.saturating_mul(buffer_scale.max(1)).max(1)
}

/// Return the initial layer-shell size request for each chrome surface.
///
/// A surface anchored to both horizontal edges must request a zero width so
/// the compositor can fill the current output.  Passing the shell's startup
/// fallback width here pins the layer to that stale width when a shell is
/// launched directly (or while output metadata is being refreshed), leaving
/// an unpainted strip on a wider DRM output.
fn initial_layer_size(kind: ChromeSurfaceKind, _width: u32, _height: u32) -> (u32, u32) {
    match kind {
        ChromeSurfaceKind::Background => (0, 0),
        ChromeSurfaceKind::Menu => (0, MENU_H),
        ChromeSurfaceKind::Dock => (0, DOCK_H),
        ChromeSurfaceKind::MenuPopup | ChromeSurfaceKind::SpacesOverview => (1, 1),
    }
}

/// Return the configured logical size for a layer surface, falling back to
/// the current output size for the full-width chrome surfaces.  Layer-shell
/// configure events normally provide a non-zero size; the fallback keeps the
/// resize path deterministic if a compositor leaves an anchored dimension at
/// zero.
fn configured_surface_size(
    kind: ChromeSurfaceKind,
    configured: Option<(u32, u32)>,
    output_w: u32,
    output_h: u32,
) -> (u32, u32) {
    configured.unwrap_or(match kind {
        ChromeSurfaceKind::Background => (output_w, output_h),
        ChromeSurfaceKind::Menu => (output_w, MENU_H),
        ChromeSurfaceKind::Dock => (output_w, DOCK_H),
        ChromeSurfaceKind::MenuPopup | ChromeSurfaceKind::SpacesOverview => (1, 1),
    })
}

/// Apply a compositor output configure to the shell runtime.
///
/// The layer-shell protocol delivers a configure to each chrome surface, but
/// the shell owns one shared widget tree.  Re-layout that tree at the new
/// physical output dimensions and explicitly mark it dirty so the next event
/// loop turn repaints every layer with the new geometry.
fn resize_runtime_for_output(
    output_size: &mut (u32, u32),
    configured_output: Option<(u32, u32)>,
    buffer_scale: u32,
    scale: f32,
    runtime: &mut UiRuntime,
) -> bool {
    let next = configured_output.unwrap_or(*output_size);
    let changed = *output_size != next;
    *output_size = next;
    runtime.resize(
        scale_surface_dimension(next.0, buffer_scale),
        scale_surface_dimension(next.1, buffer_scale),
        scale,
    );
    // UiRuntime::resize re-lays out but intentionally does not own the
    // driver's repaint policy.  A layer configure always invalidates pixels.
    runtime.mark_dirty();
    changed
}

/// Main entry: exclusive Top/Bottom chrome + Background desktop.
pub fn run_layer_desktop(content: Box<dyn Widget>, width: u32, height: u32) -> anyhow::Result<()> {
    let conn = Connection::connect_to_env().map_err(|e| anyhow!("wayland connect: {}", e))?;
    let buffer_scale = detect_shell_scale_from_env().integer_buffer_scale();

    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();
    let display_obj = conn.display();
    let _registry = display_obj.get_registry(&qh, ());

    let mut state = LayerDesktopState {
        compositor: None,
        shm: None,
        layer_shell: None,
        seat: None,
        wl_pointer: None,
        wl_keyboard: None,
        surfaces: Vec::new(),
        runtime: None,
        output_w: width,
        output_h: height,
        buffer_scale,
        running: true,
        last_pointer: (0.0, 0.0),
        pointer_kind: ChromeSurfaceKind::Background,
        modifiers: slopos_kit::event::Modifiers::NONE,
        spaces_keyboard_exclusive: false,
        popup_origin: (0.0, 0.0),
        spaces_origin: (0.0, 0.0),
        active_toplevel_mtime: None,
        spaces_state_mtime: None,
        layer_configure_pending: false,
    };

    event_queue
        .roundtrip(&mut state)
        .map_err(|e| anyhow!("registry roundtrip: {}", e))?;

    let compositor = state
        .compositor
        .clone()
        .ok_or_else(|| anyhow!("wl_compositor not found"))?;
    let layer_shell = state
        .layer_shell
        .clone()
        .ok_or_else(|| anyhow!("zwlr_layer_shell_v1 not found"))?;

    // Background — wallpaper + icons + in-shell windows
    let bg_wl = compositor.create_surface(&qh, ChromeSurfaceKind::Background);
    let bg_layer = layer_shell.get_layer_surface(
        &bg_wl,
        None,
        Layer::Background,
        "slopos-i-desktop".into(),
        &qh,
        ChromeSurfaceKind::Background,
    );
    bg_layer.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
    bg_layer.set_exclusive_zone(0);
    bg_layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    let (bg_width, bg_height) = initial_layer_size(ChromeSurfaceKind::Background, width, height);
    bg_layer.set_size(bg_width, bg_height);
    bg_wl.set_buffer_scale(buffer_scale as i32);
    bg_wl.commit();

    // Top menu — exclusive band
    let menu_wl = compositor.create_surface(&qh, ChromeSurfaceKind::Menu);
    let menu_layer = layer_shell.get_layer_surface(
        &menu_wl,
        None,
        Layer::Top,
        "slopos-i-menu".into(),
        &qh,
        ChromeSurfaceKind::Menu,
    );
    menu_layer.set_anchor(Anchor::Top | Anchor::Left | Anchor::Right);
    menu_layer.set_exclusive_zone(MENU_H as i32);
    menu_layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    let (menu_width, menu_height) = initial_layer_size(ChromeSurfaceKind::Menu, width, MENU_H);
    menu_layer.set_size(menu_width, menu_height);
    menu_wl.set_buffer_scale(buffer_scale as i32);
    menu_wl.commit();

    // Bottom dock — exclusive band
    let dock_wl = compositor.create_surface(&qh, ChromeSurfaceKind::Dock);
    let dock_layer = layer_shell.get_layer_surface(
        &dock_wl,
        None,
        Layer::Bottom,
        "slopos-i-dock".into(),
        &qh,
        ChromeSurfaceKind::Dock,
    );
    dock_layer.set_anchor(Anchor::Bottom | Anchor::Left | Anchor::Right);
    dock_layer.set_exclusive_zone(DOCK_H as i32);
    dock_layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    let (dock_width, dock_height) = initial_layer_size(ChromeSurfaceKind::Dock, width, DOCK_H);
    dock_layer.set_size(dock_width, dock_height);
    dock_wl.set_buffer_scale(buffer_scale as i32);
    dock_wl.commit();

    // Overlay popup — sized/moved when a menu is open (1×1 placeholder when closed).
    let popup_wl = compositor.create_surface(&qh, ChromeSurfaceKind::MenuPopup);
    let popup_layer = layer_shell.get_layer_surface(
        &popup_wl,
        None,
        Layer::Overlay,
        "slopos-i-menu-popup".into(),
        &qh,
        ChromeSurfaceKind::MenuPopup,
    );
    popup_layer.set_anchor(Anchor::Top | Anchor::Left);
    popup_layer.set_exclusive_zone(0);
    popup_layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    popup_layer.set_margin(0, 0, 0, 0);
    popup_layer.set_size(1, 1);
    popup_wl.set_buffer_scale(buffer_scale as i32);
    popup_wl.commit();

    // Spaces overview — compositor-above-apps overlay, sized/moved only while
    // the shell's live overview is open (1×1 placeholder when closed).
    let spaces_wl = compositor.create_surface(&qh, ChromeSurfaceKind::SpacesOverview);
    let spaces_layer = layer_shell.get_layer_surface(
        &spaces_wl,
        None,
        Layer::Overlay,
        "slopos-i-spaces-overview".into(),
        &qh,
        ChromeSurfaceKind::SpacesOverview,
    );
    spaces_layer.set_anchor(Anchor::Top | Anchor::Left);
    spaces_layer.set_exclusive_zone(0);
    spaces_layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    spaces_layer.set_margin(0, 0, 0, 0);
    spaces_layer.set_size(1, 1);
    spaces_wl.set_buffer_scale(buffer_scale as i32);
    spaces_wl.commit();

    state.surfaces = vec![
        LayerSurf {
            kind: ChromeSurfaceKind::Background,
            wl: bg_wl,
            layer: bg_layer,
            configured: None,
            renderer: None,
        },
        LayerSurf {
            kind: ChromeSurfaceKind::Menu,
            wl: menu_wl,
            layer: menu_layer,
            configured: None,
            renderer: None,
        },
        LayerSurf {
            kind: ChromeSurfaceKind::Dock,
            wl: dock_wl,
            layer: dock_layer,
            configured: None,
            renderer: None,
        },
        LayerSurf {
            kind: ChromeSurfaceKind::MenuPopup,
            wl: popup_wl,
            layer: popup_layer,
            configured: None,
            renderer: None,
        },
        LayerSurf {
            kind: ChromeSurfaceKind::SpacesOverview,
            wl: spaces_wl,
            layer: spaces_layer,
            configured: None,
            renderer: None,
        },
    ];

    // Wait until all chrome surfaces have a configure.
    for _ in 0..32 {
        event_queue
            .roundtrip(&mut state)
            .map_err(|e| anyhow!("configure roundtrip: {}", e))?;
        if state.surfaces.iter().all(|s| s.configured.is_some()) {
            break;
        }
    }
    if !state.surfaces.iter().all(|s| s.configured.is_some()) {
        return Err(anyhow!("layer surfaces did not configure"));
    }

    let display_ptr = conn.backend().display_ptr() as *mut c_void;

    for surf in &mut state.surfaces {
        let (cw, ch) = configured_surface_size(surf.kind, surf.configured, width, height);
        let (rw, rh) = (
            scale_surface_dimension(cw, buffer_scale),
            scale_surface_dimension(ch, buffer_scale),
        );
        let surface_ptr = surf.wl.id().as_ptr() as *mut c_void;
        let renderer = futures::executor::block_on(unsafe {
            RawSurfaceRenderer::new(display_ptr, surface_ptr, rw, rh)
        })
        .map_err(|e| anyhow!("RawSurfaceRenderer {:?}: {}", surf.kind, e))?;
        surf.renderer = Some(renderer);
    }

    let (desk_w, desk_h) = state
        .surfaces
        .iter()
        .find(|s| s.kind == ChromeSurfaceKind::Background)
        .and_then(|s| s.configured)
        .unwrap_or((width, height));
    state.output_w = desk_w;
    state.output_h = desk_h;

    let scale = detect_shell_scale_from_env().as_f64() as f32;
    let mut runtime = UiRuntime::new(
        content,
        scale_surface_dimension(desk_w, buffer_scale),
        scale_surface_dimension(desk_h, buffer_scale),
        scale,
    );
    runtime.with_root_content_mut(|w| {
        if let Some(desktop) = w.as_any_mut().downcast_mut::<ShellDesktop>() {
            // The shell model is also used by tests and non-layer drivers. Only
            // mark protocol chrome live after this driver has discovered and
            // configured the actual layer-shell surfaces.
            desktop.set_layer_shell_bound(true);
        }
    });
    paint_all(&mut state, &mut runtime)?;
    // The initial configure was consumed while creating the renderers.  Only
    // later protocol configures should enter the live resize reconciliation
    // path below.
    state.layer_configure_pending = false;
    state.runtime = Some(runtime);

    // Wake at most once a second for the menu clock. Wayland input/configure
    // events interrupt the poll immediately; an idle shell performs no redraw.
    while state.running {
        dispatch_with_timeout(&conn, &mut event_queue, &mut state, 1_000)?;
        let active_toplevel_changed =
            active_toplevel_mtime_changed(&mut state.active_toplevel_mtime);
        let spaces_state_changed = spaces_state_mtime_changed(&mut state.spaces_state_mtime);

        if let Some(mut runtime) = state.runtime.take() {
            if state.layer_configure_pending {
                let configured_output = state
                    .surfaces
                    .iter()
                    .find(|surface| surface.kind == ChromeSurfaceKind::Background)
                    .and_then(|surface| surface.configured);
                let mut output_size = (state.output_w, state.output_h);
                let scale = detect_shell_scale_from_env().as_f64() as f32;
                resize_runtime_for_output(
                    &mut output_size,
                    configured_output,
                    state.buffer_scale,
                    scale,
                    &mut runtime,
                );
                (state.output_w, state.output_h) = output_size;

                // Every layer receives its own configure. Resize all existing
                // swapchains from their latest configured dimensions before
                // repainting; popup/overview surfaces remain 1x1 while closed.
                let output_w = state.output_w;
                let output_h = state.output_h;
                let buffer_scale = state.buffer_scale;
                for surface in &mut state.surfaces {
                    let (width, height) = configured_surface_size(
                        surface.kind,
                        surface.configured,
                        output_w,
                        output_h,
                    );
                    if let Some(renderer) = surface.renderer.as_mut() {
                        renderer.resize(
                            scale_surface_dimension(width, buffer_scale),
                            scale_surface_dimension(height, buffer_scale),
                        );
                    }
                }
                state.layer_configure_pending = false;
            }
            runtime.tick();
            if active_toplevel_changed || spaces_state_changed {
                // The compositor publishes focus independently of shell input;
                // make the next menu-strip paint reflect that authoritative
                // client activation without repainting continuously at idle.
                runtime.mark_dirty();
            }
            if runtime.is_dirty() {
                paint_all(&mut state, &mut runtime)?;
            }
            state.runtime = Some(runtime);
        }
    }

    Ok(())
}

fn active_toplevel_mtime_changed(previous: &mut Option<SystemTime>) -> bool {
    let path = std::env::var_os("SLOPOS_ACTIVE_TOPLEVEL_FILE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
                .map(PathBuf::from)
                .map(|dir| dir.join("active-toplevel"))
        });
    let current = path
        .and_then(|path| fs::metadata(path).ok())
        .and_then(|metadata| metadata.modified().ok());
    if *previous == current {
        return false;
    }
    *previous = current;
    true
}

fn spaces_state_mtime_changed(previous: &mut Option<SystemTime>) -> bool {
    let path = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|dir| dir.join(slopos_bus::SPACES_STATE_FILE));
    let current = path
        .and_then(|path| fs::metadata(path).ok())
        .and_then(|metadata| metadata.modified().ok());
    if *previous == current {
        return false;
    }
    *previous = current;
    true
}

/// Dispatch pending Wayland events, waiting up to `timeout_ms` for new ones.
fn dispatch_with_timeout(
    conn: &Connection,
    event_queue: &mut wayland_client::EventQueue<LayerDesktopState>,
    state: &mut LayerDesktopState,
    timeout_ms: i32,
) -> anyhow::Result<()> {
    use std::os::fd::{AsFd, AsRawFd};

    event_queue
        .dispatch_pending(state)
        .map_err(|e| anyhow!("dispatch_pending: {}", e))?;
    conn.flush().map_err(|e| anyhow!("flush: {}", e))?;

    if let Some(guard) = event_queue.prepare_read() {
        #[repr(C)]
        struct PollFd {
            fd: i32,
            events: i16,
            revents: i16,
        }
        extern "C" {
            fn poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
        }
        const POLLIN: i16 = 0x0001;

        let mut pfd = PollFd {
            fd: conn.as_fd().as_raw_fd(),
            events: POLLIN,
            revents: 0,
        };
        let ready = unsafe { poll(&mut pfd, 1, timeout_ms) };
        if ready > 0 {
            let _ = guard.read();
        } else {
            // Timeout or error: cancel the prepared read without consuming.
            drop(guard);
        }
    }

    event_queue
        .dispatch_pending(state)
        .map_err(|e| anyhow!("dispatch_pending after poll: {}", e))?;
    Ok(())
}

fn paint_all(state: &mut LayerDesktopState, runtime: &mut UiRuntime) -> anyhow::Result<()> {
    let out_w = state.output_w as f32;
    let menu_h = MENU_H as f32;
    let dock_h = DOCK_H as f32;

    // Sync Overlay popup size/margins before painting (may need a configure).
    let dropdown = runtime
        .with_root_content_mut(|w| {
            w.as_any_mut()
                .downcast_mut::<ShellDesktop>()
                .and_then(|d| d.open_menu_dropdown_geo())
        })
        .flatten();
    if let Some(surf) = state
        .surfaces
        .iter_mut()
        .find(|s| s.kind == ChromeSurfaceKind::MenuPopup)
    {
        let (x, y, w, h) = dropdown.unwrap_or((0, 0, 1, 1));
        state.popup_origin = (x as f32, y as f32);
        let (cw, ch) = surf.configured.unwrap_or((1, 1));
        if cw != w || ch != h {
            surf.layer.set_margin(y, 0, 0, x);
            surf.layer.set_size(w, h);
            surf.wl.commit();
            // Configure arrives on the next dispatch; skip popup paint this frame.
        } else if let Some(renderer) = surf.renderer.as_mut() {
            renderer.resize(
                scale_surface_dimension(w, state.buffer_scale),
                scale_surface_dimension(h, state.buffer_scale),
            );
        }
    }

    // Spaces overview Overlay — keep it a separate layer above ordinary XDG
    // clients so an overview click cannot be intercepted by an app surface.
    let spaces_geo = runtime
        .with_root_content_mut(|w| {
            w.as_any_mut()
                .downcast_mut::<ShellDesktop>()
                .and_then(|d| d.workspace_overview_geo())
        })
        .flatten();
    if let Some(surf) = state
        .surfaces
        .iter_mut()
        .find(|s| s.kind == ChromeSurfaceKind::SpacesOverview)
    {
        let (x, y, w, h) = spaces_geo.unwrap_or((0, 0, 1, 1));
        state.spaces_origin = (x as f32, y as f32);
        let (cw, ch) = surf.configured.unwrap_or((1, 1));
        let wants_exclusive_keyboard = spaces_geo.is_some();
        let mut commit = false;
        if wants_exclusive_keyboard != state.spaces_keyboard_exclusive {
            surf.layer
                .set_keyboard_interactivity(if wants_exclusive_keyboard {
                    KeyboardInteractivity::Exclusive
                } else {
                    KeyboardInteractivity::OnDemand
                });
            state.spaces_keyboard_exclusive = wants_exclusive_keyboard;
            commit = true;
        }
        if cw != w || ch != h {
            surf.layer.set_margin(y, 0, 0, x);
            surf.layer.set_size(w, h);
            commit = true;
        }
        if commit {
            surf.wl.commit();
        } else if let Some(renderer) = surf.renderer.as_mut() {
            renderer.resize(
                scale_surface_dimension(w, state.buffer_scale),
                scale_surface_dimension(h, state.buffer_scale),
            );
        }
    }

    // Background
    runtime.with_root_content_mut(|w| {
        if let Some(desktop) = w.as_any_mut().downcast_mut::<ShellDesktop>() {
            desktop.set_menu_popup_origin(false);
            desktop.set_suppress_dropdown_paint(true);
            desktop.set_paint_filter(ShellPaintFilter::Background);
        }
    });
    if let Some(surf) = state
        .surfaces
        .iter_mut()
        .find(|s| s.kind == ChromeSurfaceKind::Background)
    {
        if let Some(renderer) = surf.renderer.as_mut() {
            runtime
                .paint(renderer)
                .map_err(|e| anyhow!("bg paint: {}", e))?;
            surf.wl.commit();
        }
    }

    // Menu strip (titles only — dropdown is on Overlay)
    runtime.with_root_content_mut(|w| {
        if let Some(desktop) = w.as_any_mut().downcast_mut::<ShellDesktop>() {
            desktop.set_menu_popup_origin(false);
            desktop.set_suppress_dropdown_paint(true);
            desktop.prepare_menu_strip_layout(out_w, menu_h);
            desktop.set_paint_filter(ShellPaintFilter::MenuBar);
        }
    });
    if let Some(surf) = state
        .surfaces
        .iter_mut()
        .find(|s| s.kind == ChromeSurfaceKind::Menu)
    {
        if let Some(renderer) = surf.renderer.as_mut() {
            runtime
                .paint_ex(renderer, false, false)
                .map_err(|e| anyhow!("menu paint: {}", e))?;
            surf.wl.commit();
        }
    }

    // Menu dropdown Overlay (only when open and configured)
    if dropdown.is_some() {
        let ready = state
            .surfaces
            .iter()
            .find(|s| s.kind == ChromeSurfaceKind::MenuPopup)
            .and_then(|s| s.configured)
            .map(|(w, h)| w > 1 || h > 1)
            .unwrap_or(false);
        if ready {
            runtime.with_root_content_mut(|w| {
                if let Some(desktop) = w.as_any_mut().downcast_mut::<ShellDesktop>() {
                    desktop.set_suppress_dropdown_paint(false);
                    desktop.set_menu_popup_origin(true);
                    desktop.prepare_menu_strip_layout(out_w, menu_h);
                    desktop.set_paint_filter(ShellPaintFilter::MenuPopup);
                }
            });
            if let Some(surf) = state
                .surfaces
                .iter_mut()
                .find(|s| s.kind == ChromeSurfaceKind::MenuPopup)
            {
                if let Some(renderer) = surf.renderer.as_mut() {
                    runtime
                        .paint_ex(renderer, false, false)
                        .map_err(|e| anyhow!("menu popup paint: {}", e))?;
                    surf.wl.commit();
                }
            }
        }
    }

    // Spaces overview Overlay (only while open and configured).
    if let Some((_, _, w, h)) = spaces_geo {
        let ready = state
            .surfaces
            .iter()
            .find(|s| s.kind == ChromeSurfaceKind::SpacesOverview)
            .and_then(|s| s.configured)
            .map(|(cw, ch)| cw > 1 || ch > 1)
            .unwrap_or(false);
        if ready {
            runtime.with_root_content_mut(|root| {
                if let Some(desktop) = root.as_any_mut().downcast_mut::<ShellDesktop>() {
                    desktop.prepare_workspace_overview_layout(w as f32, h as f32);
                    desktop.set_paint_filter(ShellPaintFilter::SpacesOverview);
                }
            });
            if let Some(surf) = state
                .surfaces
                .iter_mut()
                .find(|s| s.kind == ChromeSurfaceKind::SpacesOverview)
            {
                if let Some(renderer) = surf.renderer.as_mut() {
                    runtime
                        .paint_ex(renderer, false, false)
                        .map_err(|e| anyhow!("Spaces overview paint: {}", e))?;
                    surf.wl.commit();
                }
            }
        }
    }

    // Dock strip
    runtime.with_root_content_mut(|w| {
        if let Some(desktop) = w.as_any_mut().downcast_mut::<ShellDesktop>() {
            desktop.set_menu_popup_origin(false);
            desktop.set_suppress_dropdown_paint(false);
            desktop.prepare_dock_strip_layout(out_w, dock_h);
            desktop.set_paint_filter(ShellPaintFilter::Dock);
        }
    });
    if let Some(surf) = state
        .surfaces
        .iter_mut()
        .find(|s| s.kind == ChromeSurfaceKind::Dock)
    {
        if let Some(renderer) = surf.renderer.as_mut() {
            runtime
                .paint_ex(renderer, false, false)
                .map_err(|e| anyhow!("dock paint: {}", e))?;
            surf.wl.commit();
        }
    }

    // Restore full desktop layout so menu/dock hit-testing uses output coords.
    runtime.with_root_content_mut(|w| {
        if let Some(desktop) = w.as_any_mut().downcast_mut::<ShellDesktop>() {
            desktop.set_menu_popup_origin(false);
            desktop.set_suppress_dropdown_paint(false);
            desktop.set_paint_filter(ShellPaintFilter::Background);
        }
    });
    let scale = detect_shell_scale_from_env().as_f64() as f32;
    runtime.resize(
        scale_surface_dimension(state.output_w, state.buffer_scale),
        scale_surface_dimension(state.output_h, state.buffer_scale),
        scale,
    );
    // The resize above restores hit-test layout after painting menu/dock strips;
    // it does not invalidate the pixels just committed to those surfaces.
    runtime.clear_dirty();

    Ok(())
}

fn map_pointer_to_desktop(
    kind: ChromeSurfaceKind,
    surface_x: f64,
    surface_y: f64,
    output_h: u32,
    popup_origin: (f32, f32),
) -> (f32, f32) {
    match kind {
        ChromeSurfaceKind::Background | ChromeSurfaceKind::Menu => {
            (surface_x as f32, surface_y as f32)
        }
        ChromeSurfaceKind::MenuPopup => (
            popup_origin.0 + surface_x as f32,
            popup_origin.1 + surface_y as f32,
        ),
        ChromeSurfaceKind::SpacesOverview => (
            popup_origin.0 + surface_x as f32,
            popup_origin.1 + surface_y as f32,
        ),
        ChromeSurfaceKind::Dock => {
            let y = (output_h.saturating_sub(DOCK_H) as f64) + surface_y;
            (surface_x as f32, y as f32)
        }
    }
}

fn set_runtime_filter(runtime: &mut UiRuntime, filter: Option<ShellPaintFilter>) {
    runtime.with_root_content_mut(|w| {
        if let Some(desktop) = w.as_any_mut().downcast_mut::<ShellDesktop>() {
            desktop.set_input_filter(filter);
        }
    });
}

fn set_runtime_input_filter(runtime: &mut UiRuntime, kind: Option<ChromeSurfaceKind>) {
    let filter = kind.map(|kind| match kind {
        ChromeSurfaceKind::Background => ShellPaintFilter::Background,
        ChromeSurfaceKind::Menu => ShellPaintFilter::MenuBar,
        ChromeSurfaceKind::MenuPopup => ShellPaintFilter::MenuPopup,
        ChromeSurfaceKind::SpacesOverview => ShellPaintFilter::SpacesOverview,
        ChromeSurfaceKind::Dock => ShellPaintFilter::Dock,
    });
    set_runtime_filter(runtime, filter);
}

struct LayerDesktopState {
    compositor: Option<wayland_client::protocol::wl_compositor::WlCompositor>,
    shm: Option<wayland_client::protocol::wl_shm::WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    seat: Option<wayland_client::protocol::wl_seat::WlSeat>,
    wl_pointer: Option<wayland_client::protocol::wl_pointer::WlPointer>,
    wl_keyboard: Option<wl_keyboard::WlKeyboard>,
    surfaces: Vec<LayerSurf>,
    runtime: Option<UiRuntime>,
    output_w: u32,
    output_h: u32,
    buffer_scale: u32,
    running: bool,
    last_pointer: (f64, f64),
    pointer_kind: ChromeSurfaceKind,
    /// Current keyboard modifiers from wl_keyboard::Modifiers (xkb mask).
    modifiers: slopos_kit::event::Modifiers,
    /// Whether the live Spaces overlay currently requests exclusive keyboard
    /// focus from the compositor. Keep this client-side state so the layer
    /// surface is not recommitted on every paint tick.
    spaces_keyboard_exclusive: bool,
    /// Output-local origin of the menu Overlay popup surface.
    popup_origin: (f32, f32),
    /// Output-local origin of the Spaces overview Overlay surface.
    spaces_origin: (f32, f32),
    /// Last observed compositor focus-record mtime; used to repaint the
    /// global menu only when focus changes.
    active_toplevel_mtime: Option<SystemTime>,
    /// Last observed compositor Spaces projection mtime.
    spaces_state_mtime: Option<SystemTime>,
    /// Set by layer-shell Configure events and consumed after dispatch so the
    /// shared runtime and every surface swapchain are resized together.
    layer_configure_pending: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for LayerDesktopState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match &interface[..] {
                "wl_compositor" => {
                    let v = version.min(4);
                    state.compositor = Some(registry.bind(name, v, qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, 1, qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    let v = version.min(4);
                    state.layer_shell = Some(registry.bind(name, v, qh, ()));
                }
                "wl_seat" => {
                    let v = version.min(9);
                    state.seat = Some(registry.bind(name, v, qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wayland_client::protocol::wl_compositor::WlCompositor, ()> for LayerDesktopState {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_compositor::WlCompositor,
        _: wayland_client::protocol::wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wayland_client::protocol::wl_shm::WlShm, ()> for LayerDesktopState {
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_shm::WlShm,
        _: wayland_client::protocol::wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wayland_client::protocol::wl_surface::WlSurface, ChromeSurfaceKind>
    for LayerDesktopState
{
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_surface::WlSurface,
        _: wayland_client::protocol::wl_surface::Event,
        _: &ChromeSurfaceKind,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wayland_client::protocol::wl_seat::WlSeat, ()> for LayerDesktopState {
    fn event(
        state: &mut Self,
        seat: &wayland_client::protocol::wl_seat::WlSeat,
        event: wayland_client::protocol::wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        {
            if caps.contains(wayland_client::protocol::wl_seat::Capability::Pointer)
                && state.wl_pointer.is_none()
            {
                state.wl_pointer = Some(seat.get_pointer(qh, ()));
            }
            if caps.contains(wayland_client::protocol::wl_seat::Capability::Keyboard)
                && state.wl_keyboard.is_none()
            {
                state.wl_keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}

impl Dispatch<wayland_client::protocol::wl_pointer::WlPointer, ()> for LayerDesktopState {
    fn event(
        state: &mut Self,
        _: &wayland_client::protocol::wl_pointer::WlPointer,
        event: wayland_client::protocol::wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.last_pointer = (surface_x, surface_y);
                let (dx, dy) = map_pointer_to_desktop(
                    state.pointer_kind,
                    surface_x,
                    surface_y,
                    state.output_h,
                    if state.pointer_kind == ChromeSurfaceKind::SpacesOverview {
                        state.spaces_origin
                    } else {
                        state.popup_origin
                    },
                );
                if let Some(runtime) = state.runtime.as_mut() {
                    set_runtime_input_filter(runtime, Some(state.pointer_kind));
                    runtime.pointer_moved(dx, dy);
                    set_runtime_input_filter(runtime, None);
                }
            }
            wl_pointer::Event::Button {
                button,
                state: btn_state,
                ..
            } => {
                let Some(mouse) = mouse_button_from_linux(button) else {
                    return;
                };
                let pressed = matches!(btn_state, WEnum::Value(wl_pointer::ButtonState::Pressed));
                let (px, py) = state.last_pointer;
                let (dx, dy) = map_pointer_to_desktop(
                    state.pointer_kind,
                    px,
                    py,
                    state.output_h,
                    if state.pointer_kind == ChromeSurfaceKind::SpacesOverview {
                        state.spaces_origin
                    } else {
                        state.popup_origin
                    },
                );
                if let Some(runtime) = state.runtime.as_mut() {
                    set_runtime_input_filter(runtime, Some(state.pointer_kind));
                    runtime.pointer_moved(dx, dy);
                    let _ = runtime.pointer_button(mouse, pressed, now_ms());
                    set_runtime_input_filter(runtime, None);
                }
            }
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_kind = state
                    .surfaces
                    .iter()
                    .find(|s| s.wl.id() == surface.id())
                    .map(|s| s.kind)
                    .unwrap_or(ChromeSurfaceKind::Background);
                state.last_pointer = (surface_x, surface_y);
                let (dx, dy) = map_pointer_to_desktop(
                    state.pointer_kind,
                    surface_x,
                    surface_y,
                    state.output_h,
                    if state.pointer_kind == ChromeSurfaceKind::SpacesOverview {
                        state.spaces_origin
                    } else {
                        state.popup_origin
                    },
                );
                if let Some(runtime) = state.runtime.as_mut() {
                    set_runtime_input_filter(runtime, Some(state.pointer_kind));
                    runtime.pointer_moved(dx, dy);
                    runtime.set_focus(true);
                    set_runtime_input_filter(runtime, None);
                }
            }
            wl_pointer::Event::Leave { .. } => {
                if let Some(runtime) = state.runtime.as_mut() {
                    set_runtime_input_filter(runtime, Some(state.pointer_kind));
                    runtime.set_focus(false);
                    set_runtime_input_filter(runtime, None);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for LayerDesktopState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Modifiers { mods_depressed, .. } => {
                state.modifiers = modifiers_from_xkb_mask(mods_depressed);
                if let Some(runtime) = state.runtime.as_mut() {
                    runtime.set_modifiers(state.modifiers);
                }
            }
            wl_keyboard::Event::Key {
                key,
                state: key_state,
                ..
            } => {
                let Some(runtime) = state.runtime.as_mut() else {
                    return;
                };
                let Some((code, pressed)) = key_event_info_from_wayland(key, &key_state) else {
                    return;
                };
                let mods = state.modifiers;
                let ev = if pressed {
                    Event::KeyDown {
                        key: code,
                        modifiers: mods,
                    }
                } else {
                    Event::KeyUp {
                        key: code,
                        modifiers: mods,
                    }
                };
                set_runtime_filter(runtime, Some(ShellPaintFilter::All));
                runtime.key(ev);
                // Feed printable chars for lock password / text fields.
                if pressed && !mods.control && !mods.alt && !mods.meta {
                    if let Some(ch) = char_from_keycode(code, mods.shift) {
                        runtime.key(Event::Char { character: ch });
                    }
                }
                set_runtime_input_filter(runtime, None);
            }
            wl_keyboard::Event::Enter { .. } => {
                if let Some(runtime) = state.runtime.as_mut() {
                    set_runtime_filter(runtime, Some(ShellPaintFilter::All));
                    runtime.set_focus(true);
                    set_runtime_input_filter(runtime, None);
                }
            }
            wl_keyboard::Event::Leave { .. } => {
                if let Some(runtime) = state.runtime.as_mut() {
                    set_runtime_filter(runtime, Some(ShellPaintFilter::All));
                    runtime.set_focus(false);
                    set_runtime_input_filter(runtime, None);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerShellV1, ()> for LayerDesktopState {
    fn event(
        _: &mut Self,
        _: &ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ChromeSurfaceKind> for LayerDesktopState {
    fn event(
        state: &mut Self,
        surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        kind: &ChromeSurfaceKind,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            state.layer_configure_pending = true;
            let w = if width == 0 {
                match kind {
                    ChromeSurfaceKind::MenuPopup => 1,
                    ChromeSurfaceKind::SpacesOverview => 1,
                    _ => state.output_w,
                }
            } else {
                width
            };
            let h = if height == 0 {
                match kind {
                    ChromeSurfaceKind::Menu => MENU_H,
                    ChromeSurfaceKind::Dock => DOCK_H,
                    ChromeSurfaceKind::Background => state.output_h,
                    ChromeSurfaceKind::MenuPopup => 1,
                    ChromeSurfaceKind::SpacesOverview => 1,
                }
            } else {
                height
            };
            if let Some(surf) = state.surfaces.iter_mut().find(|s| s.kind == *kind) {
                surf.configured = Some((w, h));
            }
            surface.ack_configure(serial);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_kit::{Label, Rect};

    #[test]
    fn full_width_chrome_requests_compositor_sized_layers() {
        assert_eq!(
            initial_layer_size(ChromeSurfaceKind::Background, 1024, 768),
            (0, 0)
        );
        assert_eq!(
            initial_layer_size(ChromeSurfaceKind::Menu, 1024, 768),
            (0, MENU_H)
        );
        assert_eq!(
            initial_layer_size(ChromeSurfaceKind::Dock, 1024, 768),
            (0, DOCK_H)
        );
    }

    #[test]
    fn popup_layers_keep_explicit_initial_size() {
        assert_eq!(
            initial_layer_size(ChromeSurfaceKind::MenuPopup, 1280, 800),
            (1, 1)
        );
        assert_eq!(
            initial_layer_size(ChromeSurfaceKind::SpacesOverview, 1280, 800),
            (1, 1)
        );
    }

    #[test]
    fn layer_configure_resizes_runtime_and_invalidates_pixels() {
        let mut runtime = UiRuntime::new(Box::new(Label::new("resize")), 640, 480, 1.0);
        runtime.clear_dirty();
        let mut output_size = (640, 480);

        let changed =
            resize_runtime_for_output(&mut output_size, Some((1280, 720)), 1, 1.0, &mut runtime);

        assert!(changed);
        assert_eq!(output_size, (1280, 720));
        assert!(runtime.is_dirty());
        let root_rect = runtime
            .with_root_content_mut(|root| root.rect())
            .expect("runtime root");
        assert_eq!(root_rect, Rect::new(0.0, 0.0, 1280.0, 720.0));
        assert_eq!(
            configured_surface_size(ChromeSurfaceKind::Menu, None, 1280, 720),
            (1280, MENU_H)
        );
        assert_eq!(scale_surface_dimension(1280, 2), 2560);
    }

    #[test]
    fn wayland_key_uses_xkb_offset_before_evdev_lookup() {
        // Smithay emits KEY_A's evdev code (30) as the wire value 22.
        assert_eq!(keycode_from_wayland(22), Some(KeyCode::A));
        assert_eq!(keycode_from_wayland(23), Some(KeyCode::S));
        assert_eq!(keycode_from_wayland(30), Some(KeyCode::L));
    }

    #[test]
    fn wayland_key_mapping_rejects_overflow() {
        assert_eq!(keycode_from_wayland(u32::MAX), None);
    }

    #[test]
    fn wayland_key_release_uses_the_same_normalized_key() {
        assert_eq!(
            key_event_info_from_wayland(22, &WEnum::Value(wl_keyboard::KeyState::Pressed)),
            Some((KeyCode::A, true)),
        );
        assert_eq!(
            key_event_info_from_wayland(22, &WEnum::Value(wl_keyboard::KeyState::Released)),
            Some((KeyCode::A, false)),
        );
    }

    #[test]
    fn xkb_modifier_mask_maps_standard_depressed_bits() {
        let modifiers = modifiers_from_xkb_mask((1 << 0) | (1 << 2) | (1 << 3) | (1 << 6));
        assert_eq!(
            modifiers,
            slopos_kit::event::Modifiers {
                shift: true,
                control: true,
                alt: true,
                meta: true,
            }
        );
    }
}
