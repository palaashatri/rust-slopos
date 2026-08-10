// Copyright (c) 2026 Palaash Atri
// SPDX-License-Identifier: MIT

//! Native Wayland cross-client drag-and-drop protocol client for the headless
//! compositor gate.
//!
//! The source and target are separate processes. The source waits for a real
//! `wl_pointer.button` serial, starts `wl_data_device.start_drag`, and offers
//! text plus `text/uri-list`. The target accepts the offer, receives both MIME
//! payloads after `wl_data_device.drop`, and finishes the offer. A test-only
//! headless input control path drives the pointer; this is protocol evidence,
//! not physical input or third-party compatibility evidence.

use std::{
    env,
    error::Error,
    fs::OpenOptions,
    io::{Read, Write},
    os::fd::AsFd,
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer,
    wl_data_source, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

const MIME_TEXT: &str = "text/plain;charset=utf-8";
const MIME_URI: &str = "text/uri-list";
const TEXT_PAYLOAD: &[u8] = b"SLOPOS native cross-client DnD\nUTF-8: cafe\xCC\x81\n";
const URI_PAYLOAD: &[u8] = b"file:///tmp/slopos-dnd.txt\r\n";
const BTN_LEFT: u32 = 0x110;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    Source,
    Target,
    TargetAbort,
}

struct State {
    role: Role,
    toplevel_configured: bool,
    close_requested: bool,
    pointer_entered: bool,
    source_started: bool,
    source_cancelled: bool,
    source_drop_performed: bool,
    source_send_count: u32,
    target_entered: bool,
    target_left: bool,
    target_left_before_drop: bool,
    target_dropped: bool,
    target_abort_requested: bool,
    target_motion_count: u32,
    dnd_offer: Option<wl_data_offer::WlDataOffer>,
    offered_mimes: Vec<String>,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    buffer: Option<wl_buffer::WlBuffer>,
    data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    pointer: Option<wl_pointer::WlPointer>,
    data_device: Option<wl_data_device::WlDataDevice>,
    origin_surface: Option<wl_surface::WlSurface>,
    data_source: Option<wl_data_source::WlDataSource>,
    icon_surface: Option<wl_surface::WlSurface>,
}

impl State {
    fn new(role: Role) -> Self {
        Self {
            role,
            toplevel_configured: false,
            close_requested: false,
            pointer_entered: false,
            source_started: false,
            source_cancelled: false,
            source_drop_performed: false,
            source_send_count: 0,
            target_entered: false,
            target_left: false,
            target_left_before_drop: false,
            target_dropped: false,
            target_abort_requested: false,
            target_motion_count: 0,
            dnd_offer: None,
            offered_mimes: Vec::new(),
            compositor: None,
            shm: None,
            buffer: None,
            data_device_manager: None,
            pointer: None,
            data_device: None,
            origin_surface: None,
            data_source: None,
            icon_surface: None,
        }
    }
}

type CommonBindings = (
    State,
    wayland_client::EventQueue<State>,
    QueueHandle<State>,
    wl_seat::WlSeat,
    wl_data_device_manager::WlDataDeviceManager,
    xdg_wm_base::XdgWmBase,
);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(State: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(State: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(State: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(State: ignore wl_surface::WlSurface);

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &wl_data_device_manager::WlDataDeviceManager,
        _event: wl_data_device_manager::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    wayland_client::event_created_child!(State, wl_data_device::WlDataDevice, [
        0 => (wl_data_offer::WlDataOffer, ())
    ]);

    fn event(
        state: &mut Self,
        _proxy: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => state.dnd_offer = Some(id),
            wl_data_device::Event::Enter {
                serial,
                id: Some(offer),
                ..
            } if matches!(state.role, Role::Target | Role::TargetAbort) => {
                state.target_entered = true;
                state.dnd_offer = Some(offer.clone());
                offer.accept(serial, Some(MIME_TEXT.to_owned()));
                offer.set_actions(
                    wl_data_device_manager::DndAction::Copy,
                    wl_data_device_manager::DndAction::Copy,
                );
                println!("SLOPOS_DND_ENTER_VERIFIED serial={serial} mime={MIME_TEXT} action=copy");
                let _ = std::io::stdout().flush();
                if state.role == Role::TargetAbort {
                    state.target_abort_requested = true;
                    println!("SLOPOS_DND_TARGET_ABORTING");
                    let _ = std::io::stdout().flush();
                }
            }
            wl_data_device::Event::Enter { .. }
                if matches!(state.role, Role::Target | Role::TargetAbort) =>
            {
                state.target_entered = true;
            }
            wl_data_device::Event::Leave
                if matches!(state.role, Role::Target | Role::TargetAbort) =>
            {
                if !state.target_dropped {
                    state.target_left_before_drop = true;
                }
                state.target_left = true;
            }
            wl_data_device::Event::Motion { .. }
                if matches!(state.role, Role::Target | Role::TargetAbort) =>
            {
                state.target_motion_count = state.target_motion_count.saturating_add(1);
            }
            wl_data_device::Event::Drop
                if matches!(state.role, Role::Target | Role::TargetAbort) =>
            {
                state.target_dropped = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            if state.dnd_offer.as_ref().is_none_or(|offer| offer == proxy) {
                state.dnd_offer = Some(proxy.clone());
                state.offered_mimes.push(mime_type);
            }
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for State {
    fn event(
        state: &mut Self,
        _proxy: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let data = match mime_type.as_str() {
                    MIME_TEXT => Some(TEXT_PAYLOAD),
                    MIME_URI => Some(URI_PAYLOAD),
                    _ => None,
                };
                if let Some(data) = data {
                    let mut file = std::fs::File::from(fd);
                    match file.write_all(data) {
                        Ok(()) => {
                            let _ = file.flush();
                            state.source_send_count = state.source_send_count.saturating_add(1);
                            println!(
                                "SLOPOS_DND_SOURCE_SENT mime={mime_type} bytes={}",
                                data.len()
                            );
                            let _ = std::io::stdout().flush();
                        }
                        Err(error) => {
                            println!(
                                "SLOPOS_DND_SOURCE_SEND_FAILED mime={mime_type} error={error}"
                            );
                            let _ = std::io::stdout().flush();
                        }
                    }
                }
            }
            wl_data_source::Event::Cancelled => {
                state.source_cancelled = true;
                println!("SLOPOS_DND_SOURCE_CANCELLED");
                let _ = std::io::stdout().flush();
            }
            wl_data_source::Event::DndDropPerformed => {
                state.source_drop_performed = true;
                println!("SLOPOS_DND_SOURCE_DROP_PERFORMED");
                let _ = std::io::stdout().flush();
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _proxy: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _data: &(),
        connection: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { .. } => state.pointer_entered = true,
            wl_pointer::Event::Button {
                serial,
                button,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } if state.role == Role::Source && button == BTN_LEFT && !state.source_started => {
                if !state.pointer_entered {
                    println!("SLOPOS_DND_SOURCE_BUTTON_WITHOUT_ENTER serial={serial}");
                    let _ = std::io::stdout().flush();
                    return;
                }
                let Some(manager) = state.data_device_manager.as_ref() else {
                    return;
                };
                let Some(compositor) = state.compositor.as_ref() else {
                    return;
                };
                let Some(data_device) = state.data_device.as_ref() else {
                    return;
                };
                let Some(origin) = state.origin_surface.as_ref() else {
                    return;
                };
                let source = manager.create_data_source(queue_handle, ());
                source.offer(MIME_TEXT.to_owned());
                source.offer(MIME_URI.to_owned());
                source.set_actions(wl_data_device_manager::DndAction::Copy);
                let icon = compositor.create_surface(queue_handle, ());
                data_device.start_drag(Some(&source), origin, Some(&icon), serial);
                state.data_source = Some(source);
                state.icon_surface = Some(icon);
                state.source_started = true;
                println!("SLOPOS_DND_SOURCE_STARTED serial={serial} icon=true");
                let _ = std::io::stdout().flush();
                let _ = connection.flush();
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _state: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.toplevel_configured = true;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _proxy: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        if matches!(event, xdg_toplevel::Event::Close) {
            state.close_requested = true;
        }
    }
}

fn create_toplevel(
    compositor: &wl_compositor::WlCompositor,
    wm_base: &xdg_wm_base::XdgWmBase,
    queue_handle: &QueueHandle<State>,
    title: &str,
) -> (
    wl_surface::WlSurface,
    xdg_surface::XdgSurface,
    xdg_toplevel::XdgToplevel,
) {
    let surface = compositor.create_surface(queue_handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, queue_handle, ());
    let toplevel = xdg_surface.get_toplevel(queue_handle, ());
    toplevel.set_title(title.to_owned());
    toplevel.set_app_id("io.github.palaashatri.slopos.dnd-smoke".to_owned());
    surface.commit();
    (surface, xdg_surface, toplevel)
}

fn create_test_buffer(
    shm: &wl_shm::WlShm,
    queue_handle: &QueueHandle<State>,
) -> Result<wl_buffer::WlBuffer, Box<dyn Error>> {
    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 240;
    let size = WIDTH
        .checked_mul(HEIGHT)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("SHM buffer size overflow")?;
    let path = env::temp_dir().join(format!("slopos-dnd-shm-{}-{}", std::process::id(), size));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.set_len(u64::from(size))?;
    let pool = shm.create_pool(file.as_fd(), size as i32, queue_handle, ());
    let buffer = pool.create_buffer(
        0,
        WIDTH as i32,
        HEIGHT as i32,
        (WIDTH * 4) as i32,
        wl_shm::Format::Argb8888,
        queue_handle,
        (),
    );
    drop(pool);
    drop(file);
    let _ = std::fs::remove_file(path);
    Ok(buffer)
}

fn wait_for_toplevel(
    event_queue: &mut wayland_client::EventQueue<State>,
    state: &mut State,
) -> Result<(), Box<dyn Error>> {
    while !state.toplevel_configured {
        if state.close_requested {
            return Err("compositor closed DnD toplevel before configure".into());
        }
        event_queue.blocking_dispatch(state)?;
    }
    Ok(())
}

fn bind_common(connection: &Connection, role: Role) -> Result<CommonBindings, Box<dyn Error>> {
    let (globals, event_queue) = registry_queue_init::<State>(connection)?;
    let queue_handle = event_queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let shm = globals.bind::<wl_shm::WlShm, _, _>(&queue_handle, 1..=1, ())?;
    let wm_base = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&queue_handle, 1..=6, ())?;
    let manager = globals.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
        &queue_handle,
        1..=3,
        (),
    )?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let state = State {
        compositor: Some(compositor),
        shm: Some(shm),
        data_device_manager: Some(manager.clone()),
        ..State::new(role)
    };
    Ok((state, event_queue, queue_handle, seat, manager, wm_base))
}

fn run_source(connection: &Connection) -> Result<(), Box<dyn Error>> {
    let (mut state, mut event_queue, queue_handle, seat, manager, wm_base) =
        bind_common(connection, Role::Source)?;
    let compositor = state.compositor.clone().ok_or("missing compositor")?;
    state.pointer = Some(seat.get_pointer(&queue_handle, ()));
    let data_device = manager.get_data_device(&seat, &queue_handle, ());
    let (surface, _xdg_surface, _toplevel) =
        create_toplevel(&compositor, &wm_base, &queue_handle, "SLOPOS DnD source");
    state.data_device = Some(data_device);
    state.origin_surface = Some(surface.clone());
    connection.flush()?;
    wait_for_toplevel(&mut event_queue, &mut state)?;
    let shm = state.shm.clone().ok_or("missing wl_shm")?;
    let buffer = create_test_buffer(&shm, &queue_handle)?;
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, 320, 240);
    surface.commit();
    state.buffer = Some(buffer);
    connection.flush()?;
    println!("SLOPOS_DND_SOURCE_READY");
    std::io::stdout().flush()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        event_queue.blocking_dispatch(&mut state)?;
        if state.source_cancelled {
            return Ok(());
        }
        if state.source_drop_performed && state.source_send_count >= 2 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "DnD source did not complete both MIME sends: dropped={} sends={}",
                state.source_drop_performed, state.source_send_count
            )
            .into());
        }
    }
}

fn read_offer(
    connection: &Connection,
    offer: &wl_data_offer::WlDataOffer,
    mime_type: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let (mut reader, writer) = UnixStream::pair()?;
    reader.set_read_timeout(Some(Duration::from_secs(10)))?;
    offer.receive(mime_type.to_owned(), writer.as_fd());
    connection.flush()?;
    drop(writer);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn run_target(connection: &Connection, abort_after_enter: bool) -> Result<(), Box<dyn Error>> {
    let (mut state, mut event_queue, queue_handle, seat, manager, wm_base) = bind_common(
        connection,
        if abort_after_enter {
            Role::TargetAbort
        } else {
            Role::Target
        },
    )?;
    let compositor = state.compositor.clone().ok_or("missing compositor")?;
    let data_device = manager.get_data_device(&seat, &queue_handle, ());
    let (surface, _xdg_surface, _toplevel) =
        create_toplevel(&compositor, &wm_base, &queue_handle, "SLOPOS DnD target");
    state.data_device = Some(data_device);
    connection.flush()?;
    wait_for_toplevel(&mut event_queue, &mut state)?;
    let shm = state.shm.clone().ok_or("missing wl_shm")?;
    let buffer = create_test_buffer(&shm, &queue_handle)?;
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, 320, 240);
    surface.commit();
    state.buffer = Some(buffer);
    connection.flush()?;
    println!("SLOPOS_DND_TARGET_READY");
    std::io::stdout().flush()?;
    while !state.target_dropped && !state.target_abort_requested {
        event_queue.blocking_dispatch(&mut state)?;
        if state.close_requested {
            return Err("compositor closed DnD target".into());
        }
    }
    if state.target_abort_requested {
        println!("SLOPOS_DND_TARGET_DISCONNECTED");
        std::io::stdout().flush()?;
        return Ok(());
    }
    if !state.target_entered || state.target_left_before_drop || state.target_motion_count == 0 {
        return Err(format!(
            "invalid target DnD lifecycle: entered={} left={} left_before_drop={} motion={} dropped={}",
            state.target_entered,
            state.target_left,
            state.target_left_before_drop,
            state.target_motion_count,
            state.target_dropped
        )
        .into());
    }
    let offer = state.dnd_offer.clone().ok_or("drop had no data offer")?;
    if !state.offered_mimes.iter().any(|mime| mime == MIME_TEXT)
        || !state.offered_mimes.iter().any(|mime| mime == MIME_URI)
    {
        return Err(format!(
            "DnD offer missing text/URI MIME types: {:?}",
            state.offered_mimes
        )
        .into());
    }
    println!(
        "SLOPOS_DND_OFFER_VERIFIED mimes={}",
        state.offered_mimes.join(",")
    );
    let text = read_offer(connection, &offer, MIME_TEXT)?;
    if text != TEXT_PAYLOAD {
        return Err(format!("DnD text payload mismatch: {} bytes", text.len()).into());
    }
    println!("SLOPOS_DND_TEXT_TRANSFER_VERIFIED bytes={}", text.len());
    let uri = read_offer(connection, &offer, MIME_URI)?;
    if uri != URI_PAYLOAD {
        return Err(format!("DnD URI payload mismatch: {} bytes", uri.len()).into());
    }
    println!("SLOPOS_DND_URI_TRANSFER_VERIFIED bytes={}", uri.len());
    offer.finish();
    offer.destroy();
    std::io::stdout().flush()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mode = env::args()
        .nth(1)
        .ok_or("usage: headless_dnd_client <source|target|target-abort>")?;
    let connection = Connection::connect_to_env()?;
    match mode.as_str() {
        "source" => run_source(&connection),
        "target" => run_target(&connection, false),
        "target-abort" => run_target(&connection, true),
        _ => Err("usage: headless_dnd_client <source|target|target-abort>".into()),
    }
}
