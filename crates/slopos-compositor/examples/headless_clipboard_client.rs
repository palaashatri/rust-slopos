// Copyright (c) 2026 Palaash Atri
// SPDX-License-Identifier: MIT

//! Native Wayland clipboard and primary-selection clients used by the compositor
//! runtime gate.
//!
//! The source and sink are separate client processes. The source creates a real
//! `wl_data_source` or `zwp_primary_selection_source_v1`, offers text MIME
//! types, and keeps its focused toplevel alive. The sink creates another
//! toplevel, receives the selection offer, reads the exact payload, and verifies
//! that an unsupported MIME request terminates with EOF. This is protocol/runtime
//! evidence only; it is not GTK, Qt, XWayland, physical-input, successful
//! cross-client DnD or hardware compatibility evidence. The DnD mode is limited
//! to checking that an invalid serial is rejected without entering a drag.

use std::{
    env,
    error::Error,
    io::{Read, Write},
    os::fd::AsFd,
    os::unix::net::UnixStream,
    thread,
    time::Duration,
};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{
    wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
    wl_registry, wl_seat, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::primary_selection::zv1::client::{
    zwp_primary_selection_device_manager_v1, zwp_primary_selection_device_v1,
    zwp_primary_selection_offer_v1, zwp_primary_selection_source_v1,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

const MIME_TEXT_UTF8: &str = "text/plain;charset=utf-8";
const MIME_TEXT: &str = "text/plain";
const MIME_LARGE: &str = "application/x-slopos-large";
const MIME_MISSING: &str = "application/x-slopos-missing";
const PRIMARY_MIME_TEXT_UTF8: &str = "text/plain;charset=utf-8";
const PRIMARY_MIME_TEXT: &str = "text/plain";
const PRIMARY_MIME_MISSING: &str = "application/x-slopos-primary-missing";
const PAYLOAD: &[u8] = b"SLOPOS native clipboard transfer\nUTF-8: cafe\xCC\x81\n";
const PRIMARY_PAYLOAD: &[u8] = b"SLOPOS native primary selection\nUTF-8: cafe\xCC\x81\n";
const LARGE_PAYLOAD_SIZE: usize = 1024 * 1024;
static LARGE_PAYLOAD: [u8; LARGE_PAYLOAD_SIZE] = [b'L'; LARGE_PAYLOAD_SIZE];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Source,
    SourceOnce,
    Sink,
    SinkAfterSourceDeath,
    SinkAbort,
    PrimarySource,
    PrimarySink,
    DndInvalidSerial,
}

#[derive(Default)]
struct State {
    toplevel_configured: bool,
    close_requested: bool,
    source_cancelled_reported: bool,
    data_offer: Option<wl_data_offer::WlDataOffer>,
    offered_mimes: Vec<String>,
    selection_received: bool,
    selection_cleared: bool,
    source_send_count: u32,
    primary_offer: Option<zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1>,
    primary_offered_mimes: Vec<String>,
    primary_selection_received: bool,
    primary_source_send_count: u32,
    dnd_entered: bool,
    dnd_left: bool,
    dnd_motion_count: u32,
    dnd_dropped: bool,
}

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
wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: ignore wl_surface::WlSurface);

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
            wl_data_device::Event::DataOffer { id } => state.data_offer = Some(id),
            wl_data_device::Event::Selection { id: Some(id) } => {
                state.data_offer = Some(id);
                state.selection_received = true;
            }
            wl_data_device::Event::Selection { id: None } => {
                state.data_offer = None;
                state.selection_cleared = true;
            }
            wl_data_device::Event::Enter { .. } => state.dnd_entered = true,
            wl_data_device::Event::Leave => state.dnd_left = true,
            wl_data_device::Event::Motion { .. } => {
                state.dnd_motion_count = state.dnd_motion_count.saturating_add(1)
            }
            wl_data_device::Event::Drop => state.dnd_dropped = true,
            _ => {}
        }
    }
}

impl Dispatch<zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1, ()>
    for State
{
    fn event(
        _state: &mut State,
        _proxy: &zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
        _event: zwp_primary_selection_device_manager_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1, ()> for State {
    wayland_client::event_created_child!(State, zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1, [
        0 => (zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1, ())
    ]);

    fn event(
        state: &mut State,
        _proxy: &zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
        event: zwp_primary_selection_device_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<State>,
    ) {
        match event {
            zwp_primary_selection_device_v1::Event::DataOffer { offer } => {
                state.primary_offer = Some(offer)
            }
            zwp_primary_selection_device_v1::Event::Selection { id: Some(offer) } => {
                state.primary_offer = Some(offer);
                state.primary_selection_received = true;
            }
            zwp_primary_selection_device_v1::Event::Selection { id: None } => {
                state.primary_offer = None;
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1, ()> for State {
    fn event(
        state: &mut State,
        proxy: &zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
        event: zwp_primary_selection_offer_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<State>,
    ) {
        if let zwp_primary_selection_offer_v1::Event::Offer { mime_type } = event {
            if state
                .primary_offer
                .as_ref()
                .is_none_or(|offer| offer == proxy)
            {
                state.primary_offer = Some(proxy.clone());
                state.primary_offered_mimes.push(mime_type);
            }
        }
    }
}

impl Dispatch<zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1, ()> for State {
    fn event(
        state: &mut State,
        _proxy: &zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
        event: zwp_primary_selection_source_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<State>,
    ) {
        if let zwp_primary_selection_source_v1::Event::Send { mime_type, fd } = event {
            let mut file = std::fs::File::from(fd);
            let data = match mime_type.as_str() {
                PRIMARY_MIME_TEXT_UTF8 | PRIMARY_MIME_TEXT => Some(PRIMARY_PAYLOAD),
                _ => None,
            };
            if let Some(data) = data {
                let _ = file.write_all(data);
                let _ = file.flush();
                state.primary_source_send_count = state.primary_source_send_count.saturating_add(1);
                println!(
                    "SLOPOS_PRIMARY_SELECTION_SOURCE_SENT mime={mime_type} bytes={}",
                    data.len()
                );
                let _ = std::io::stdout().flush();
            }
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
            if state.data_offer.as_ref().is_none_or(|offer| offer == proxy) {
                state.data_offer = Some(proxy.clone());
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
                let mut file = std::fs::File::from(fd);
                let data = match mime_type.as_str() {
                    MIME_TEXT_UTF8 | MIME_TEXT => Some(PAYLOAD),
                    MIME_LARGE => Some(&LARGE_PAYLOAD[..]),
                    _ => None,
                };
                if let Some(data) = data {
                    match file.write_all(data) {
                        Ok(()) => {
                            let _ = file.flush();
                            state.source_send_count = state.source_send_count.saturating_add(1);
                            println!(
                                "SLOPOS_CLIPBOARD_SOURCE_SENT mime={mime_type} bytes={}",
                                data.len()
                            );
                            let _ = std::io::stdout().flush();
                        }
                        Err(err)
                            if matches!(
                                err.kind(),
                                std::io::ErrorKind::BrokenPipe
                                    | std::io::ErrorKind::ConnectionReset
                            ) =>
                        {
                            println!(
                                "SLOPOS_SELECTION_TARGET_DISCONNECTED mime={mime_type} error={err}"
                            );
                            let _ = std::io::stdout().flush();
                        }
                        Err(err) => {
                            println!(
                                "SLOPOS_CLIPBOARD_SOURCE_SEND_FAILED mime={mime_type} error={err}"
                            );
                            let _ = std::io::stdout().flush();
                        }
                    }
                }
                // Unsupported MIME requests intentionally receive EOF by closing
                // the compositor-provided fd without writing any bytes.
            }
            wl_data_source::Event::Cancelled if !state.source_cancelled_reported => {
                state.source_cancelled_reported = true;
                println!("SLOPOS_CLIPBOARD_SOURCE_CANCELLED");
                let _ = std::io::stdout().flush();
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
        _toplevel: &xdg_toplevel::XdgToplevel,
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
    let wl_surface = compositor.create_surface(queue_handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&wl_surface, queue_handle, ());
    let toplevel = xdg_surface.get_toplevel(queue_handle, ());
    toplevel.set_title(title.to_owned());
    toplevel.set_app_id("io.github.palaashatri.slopos.clipboard-smoke".to_owned());
    wl_surface.commit();
    (wl_surface, xdg_surface, toplevel)
}

fn wait_for_toplevel(
    event_queue: &mut wayland_client::EventQueue<State>,
    state: &mut State,
) -> Result<(), Box<dyn Error>> {
    while !state.toplevel_configured {
        if state.close_requested {
            return Err("compositor closed clipboard toplevel before configure".into());
        }
        event_queue.blocking_dispatch(state)?;
    }
    Ok(())
}

fn run_source(connection: &Connection, keep_alive: bool) -> Result<(), Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init::<State>(connection)?;
    let queue_handle = event_queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let wm_base = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&queue_handle, 1..=6, ())?;
    let manager = globals.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
        &queue_handle,
        1..=3,
        (),
    )?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let data_device = manager.get_data_device(&seat, &queue_handle, ());
    let source = manager.create_data_source(&queue_handle, ());
    source.offer(MIME_TEXT_UTF8.to_owned());
    source.offer(MIME_TEXT.to_owned());
    source.offer(MIME_LARGE.to_owned());

    let (surface, _xdg_surface, _toplevel) = create_toplevel(
        &compositor,
        &wm_base,
        &queue_handle,
        "SLOPOS clipboard source",
    );
    connection.flush()?;
    let mut state = State::default();
    wait_for_toplevel(&mut event_queue, &mut state)?;
    surface.commit();
    data_device.set_selection(Some(&source), 0);
    connection.flush()?;
    println!("SLOPOS_CLIPBOARD_SOURCE_READY offers={MIME_TEXT_UTF8},{MIME_TEXT},{MIME_LARGE}");
    std::io::stdout().flush()?;

    if !keep_alive {
        // Let the compositor consume SetSelection before this source dies. The
        // sink launched afterwards must observe that the selection was cleared.
        thread::sleep(Duration::from_millis(500));
        return Ok(());
    }

    loop {
        event_queue.blocking_dispatch(&mut state)?;
    }
}

fn run_primary_source(connection: &Connection) -> Result<(), Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init::<State>(connection)?;
    let queue_handle = event_queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let wm_base = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&queue_handle, 1..=6, ())?;
    let manager = globals
        .bind::<zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1, _, _>(
        &queue_handle,
        1..=1,
        (),
    )?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let primary_device = manager.get_device(&seat, &queue_handle, ());
    let source = manager.create_source(&queue_handle, ());
    source.offer(PRIMARY_MIME_TEXT_UTF8.to_owned());
    source.offer(PRIMARY_MIME_TEXT.to_owned());

    let (surface, _xdg_surface, _toplevel) = create_toplevel(
        &compositor,
        &wm_base,
        &queue_handle,
        "SLOPOS primary selection source",
    );
    connection.flush()?;
    let mut state = State::default();
    wait_for_toplevel(&mut event_queue, &mut state)?;
    surface.commit();
    primary_device.set_selection(Some(&source), 0);
    connection.flush()?;
    println!(
        "SLOPOS_PRIMARY_SELECTION_SOURCE_READY offers={PRIMARY_MIME_TEXT_UTF8},{PRIMARY_MIME_TEXT}"
    );
    std::io::stdout().flush()?;

    loop {
        event_queue.blocking_dispatch(&mut state)?;
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

fn read_primary_offer(
    connection: &Connection,
    offer: &zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
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

fn run_sink(connection: &Connection, expect_selection: bool) -> Result<(), Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init::<State>(connection)?;
    let queue_handle = event_queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let wm_base = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&queue_handle, 1..=6, ())?;
    let manager = globals.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
        &queue_handle,
        1..=3,
        (),
    )?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let _data_device = manager.get_data_device(&seat, &queue_handle, ());
    let (surface, _xdg_surface, _toplevel) = create_toplevel(
        &compositor,
        &wm_base,
        &queue_handle,
        "SLOPOS clipboard sink",
    );
    connection.flush()?;
    let mut state = State::default();
    wait_for_toplevel(&mut event_queue, &mut state)?;
    surface.commit();
    connection.flush()?;

    if !expect_selection {
        for _ in 0..20 {
            event_queue.dispatch_pending(&mut state)?;
            if state.selection_received {
                return Err("clipboard selection survived source disconnect".into());
            }
            if state.selection_cleared {
                println!("SLOPOS_CLIPBOARD_SOURCE_DEATH_CLEARED observed=true");
                std::io::stdout().flush()?;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        return Err("clipboard source disconnect did not emit selection clear".into());
    }

    while !state.selection_received {
        if state.close_requested {
            return Err("compositor closed clipboard sink before selection".into());
        }
        event_queue.blocking_dispatch(&mut state)?;
    }
    let offer = state
        .data_offer
        .clone()
        .ok_or("selection event did not include a data offer")?;
    if !state
        .offered_mimes
        .iter()
        .any(|mime| mime == MIME_TEXT_UTF8)
        || !state.offered_mimes.iter().any(|mime| mime == MIME_TEXT)
        || !state.offered_mimes.iter().any(|mime| mime == MIME_LARGE)
    {
        return Err(format!(
            "clipboard offer missing expected MIME types: {:?}",
            state.offered_mimes
        )
        .into());
    }
    println!(
        "SLOPOS_CLIPBOARD_OFFER_VERIFIED mimes={}",
        state.offered_mimes.join(",")
    );

    let bytes = read_offer(connection, &offer, MIME_TEXT_UTF8)?;
    if bytes != PAYLOAD {
        return Err(format!("clipboard payload mismatch: got {} bytes", bytes.len()).into());
    }
    println!("SLOPOS_CLIPBOARD_TRANSFER_VERIFIED bytes={}", bytes.len());

    let large = read_offer(connection, &offer, MIME_LARGE)?;
    if large.len() != LARGE_PAYLOAD_SIZE || large.iter().any(|byte| *byte != b'L') {
        return Err(format!(
            "large clipboard payload mismatch: got {} bytes",
            large.len()
        )
        .into());
    }
    println!(
        "SLOPOS_CLIPBOARD_LARGE_TRANSFER_VERIFIED bytes={}",
        large.len()
    );

    let missing = read_offer(connection, &offer, MIME_MISSING)?;
    if !missing.is_empty() {
        return Err(format!(
            "unsupported MIME returned {} bytes instead of EOF",
            missing.len()
        )
        .into());
    }
    println!("SLOPOS_CLIPBOARD_MISSING_MIME_EOF_VERIFIED mime={MIME_MISSING}");
    Ok(())
}

fn run_sink_abort(connection: &Connection) -> Result<(), Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init::<State>(connection)?;
    let queue_handle = event_queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let wm_base = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&queue_handle, 1..=6, ())?;
    let manager = globals.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
        &queue_handle,
        1..=3,
        (),
    )?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let _data_device = manager.get_data_device(&seat, &queue_handle, ());
    let (surface, _xdg_surface, _toplevel) = create_toplevel(
        &compositor,
        &wm_base,
        &queue_handle,
        "SLOPOS clipboard sink abort",
    );
    connection.flush()?;
    let mut state = State::default();
    wait_for_toplevel(&mut event_queue, &mut state)?;
    surface.commit();
    connection.flush()?;

    while !state.selection_received {
        if state.close_requested {
            return Err("compositor closed clipboard abort sink before selection".into());
        }
        event_queue.blocking_dispatch(&mut state)?;
    }
    let offer = state
        .data_offer
        .clone()
        .ok_or("selection event did not include a data offer")?;
    if !state.offered_mimes.iter().any(|mime| mime == MIME_LARGE) {
        return Err(format!(
            "clipboard abort offer missing large MIME type: {:?}",
            state.offered_mimes
        )
        .into());
    }

    // Read one bounded chunk, then close the receiving side. This is honest
    // target-death evidence: the large transfer was observed partially, not
    // reported as a successful full payload after the target disappeared.
    let (mut reader, writer) = UnixStream::pair()?;
    reader.set_read_timeout(Some(Duration::from_secs(10)))?;
    offer.receive(MIME_LARGE.to_owned(), writer.as_fd());
    connection.flush()?;
    drop(writer);
    let mut partial = [0u8; 4096];
    let bytes_read = reader.read(&mut partial)?;
    if bytes_read == 0 || bytes_read >= LARGE_PAYLOAD_SIZE {
        return Err(format!(
            "clipboard abort did not observe a partial large transfer: {bytes_read} bytes"
        )
        .into());
    }
    drop(reader);
    // Let the compositor's detached writer observe the closed reader and emit
    // its EPIPE/ECONNRESET marker before this client exits.
    thread::sleep(Duration::from_millis(100));
    println!("SLOPOS_CLIPBOARD_TARGET_DEATH_RECOVERED bytes={bytes_read} closed_reader=true");
    std::io::stdout().flush()?;
    Ok(())
}

fn run_primary_sink(connection: &Connection) -> Result<(), Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init::<State>(connection)?;
    let queue_handle = event_queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let wm_base = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&queue_handle, 1..=6, ())?;
    let manager = globals
        .bind::<zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1, _, _>(
        &queue_handle,
        1..=1,
        (),
    )?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let _primary_device = manager.get_device(&seat, &queue_handle, ());
    let (surface, _xdg_surface, _toplevel) = create_toplevel(
        &compositor,
        &wm_base,
        &queue_handle,
        "SLOPOS primary selection sink",
    );
    connection.flush()?;
    let mut state = State::default();
    wait_for_toplevel(&mut event_queue, &mut state)?;
    surface.commit();
    connection.flush()?;

    while !state.primary_selection_received {
        if state.close_requested {
            return Err("compositor closed primary-selection sink before selection".into());
        }
        event_queue.blocking_dispatch(&mut state)?;
    }
    let offer = state
        .primary_offer
        .clone()
        .ok_or("primary selection event did not include a data offer")?;
    if !state
        .primary_offered_mimes
        .iter()
        .any(|mime| mime == PRIMARY_MIME_TEXT_UTF8)
        || !state
            .primary_offered_mimes
            .iter()
            .any(|mime| mime == PRIMARY_MIME_TEXT)
    {
        return Err(format!(
            "primary selection offer missing expected MIME types: {:?}",
            state.primary_offered_mimes
        )
        .into());
    }
    println!(
        "SLOPOS_PRIMARY_SELECTION_OFFER_VERIFIED mimes={}",
        state.primary_offered_mimes.join(",")
    );

    let bytes = read_primary_offer(connection, &offer, PRIMARY_MIME_TEXT_UTF8)?;
    if bytes != PRIMARY_PAYLOAD {
        return Err(format!(
            "primary selection payload mismatch: got {} bytes",
            bytes.len()
        )
        .into());
    }
    println!(
        "SLOPOS_PRIMARY_SELECTION_TRANSFER_VERIFIED bytes={}",
        bytes.len()
    );

    let missing = read_primary_offer(connection, &offer, PRIMARY_MIME_MISSING)?;
    if !missing.is_empty() {
        return Err(format!(
            "primary selection unsupported MIME returned {} bytes instead of EOF",
            missing.len()
        )
        .into());
    }
    println!("SLOPOS_PRIMARY_SELECTION_MISSING_MIME_EOF_VERIFIED mime={PRIMARY_MIME_MISSING}");
    std::io::stdout().flush()?;
    Ok(())
}

fn run_dnd_invalid_serial(connection: &Connection) -> Result<(), Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init::<State>(connection)?;
    let queue_handle = event_queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let manager = globals.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
        &queue_handle,
        1..=3,
        (),
    )?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let data_device = manager.get_data_device(&seat, &queue_handle, ());
    let source = manager.create_data_source(&queue_handle, ());
    source.offer(MIME_TEXT_UTF8.to_owned());

    // An unmapped origin is sufficient for the invalid-serial rejection path
    // and does not consume a compositor window-cascade position.
    let surface = compositor.create_surface(&queue_handle, ());
    let mut state = State::default();

    // Smithay requires this serial to identify a real pointer/touch implicit
    // grab. Headless has no input source, so serial 0 must be ignored and must
    // not synthesize a DnD enter/drop sequence.
    data_device.start_drag(Some(&source), &surface, None, 0);
    connection.flush()?;
    event_queue.roundtrip(&mut state)?;
    if state.dnd_entered || state.dnd_left || state.dnd_dropped || state.dnd_motion_count != 0 {
        return Err(format!(
            "invalid DnD serial generated events: entered={} motion={} dropped={} left={}",
            state.dnd_entered, state.dnd_motion_count, state.dnd_dropped, state.dnd_left
        )
        .into());
    }
    println!("SLOPOS_DND_INVALID_SERIAL_REJECTED serial=0 events=none compositor_safe=true");
    std::io::stdout().flush()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mode = match env::args().nth(1).as_deref() {
        Some("source") => Mode::Source,
        Some("source-once") => Mode::SourceOnce,
        Some("sink") => Mode::Sink,
        Some("sink-after-source-death") => Mode::SinkAfterSourceDeath,
        Some("sink-abort") => Mode::SinkAbort,
        Some("primary-source") => Mode::PrimarySource,
        Some("primary-sink") => Mode::PrimarySink,
        Some("dnd-invalid-serial") => Mode::DndInvalidSerial,
        _ => return Err(
            "usage: headless_clipboard_client <source|source-once|sink|sink-after-source-death|sink-abort|primary-source|primary-sink|dnd-invalid-serial>"
                .into(),
        ),
    };
    let connection = Connection::connect_to_env()?;
    match mode {
        Mode::Source => run_source(&connection, true),
        Mode::SourceOnce => run_source(&connection, false),
        Mode::Sink => run_sink(&connection, true),
        Mode::SinkAfterSourceDeath => run_sink(&connection, false),
        Mode::SinkAbort => run_sink_abort(&connection),
        Mode::PrimarySource => run_primary_source(&connection),
        Mode::PrimarySink => run_primary_sink(&connection),
        Mode::DndInvalidSerial => run_dnd_invalid_serial(&connection),
    }
}
