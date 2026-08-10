// Copyright (c) 2026 Palaash Atri
// SPDX-License-Identifier: MIT

//! Headless protocol lifecycle client for pointer-constraints-unstable-v1.
//!
//! This deliberately does not claim pointer movement enforcement: the headless
//! compositor has no physical input device. It proves that the exact compositor
//! accepts, creates, commits and destroys both lock and confinement objects.

use std::error::Error;

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_compositor, wl_pointer, wl_registry, wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::pointer_constraints::zv1::client::{
    zwp_confined_pointer_v1, zwp_locked_pointer_v1, zwp_pointer_constraints_v1,
};

#[derive(Default)]
struct State;

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
wayland_client::delegate_noop!(State: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: ignore wl_pointer::WlPointer);
wayland_client::delegate_noop!(State: ignore zwp_pointer_constraints_v1::ZwpPointerConstraintsV1);
wayland_client::delegate_noop!(State: ignore zwp_locked_pointer_v1::ZwpLockedPointerV1);
wayland_client::delegate_noop!(State: ignore zwp_confined_pointer_v1::ZwpConfinedPointerV1);

fn main() -> Result<(), Box<dyn Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init::<State>(&connection)?;
    let qh = event_queue.handle();
    let mut state = State;

    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=6, ())?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())?;
    let constraints = globals.bind::<zwp_pointer_constraints_v1::ZwpPointerConstraintsV1, _, _>(
        &qh,
        1..=1,
        (),
    )?;
    let pointer = seat.get_pointer(&qh, ());
    let surface = compositor.create_surface(&qh, ());

    let locked = constraints.lock_pointer(
        &surface,
        &pointer,
        None,
        zwp_pointer_constraints_v1::Lifetime::Persistent,
        &qh,
        (),
    );
    surface.commit();
    event_queue.roundtrip(&mut state)?;
    println!("SLOPOS_POINTER_LOCK_REQUEST_ACCEPTED persistent=1");
    locked.destroy();
    event_queue.roundtrip(&mut state)?;

    let confined = constraints.confine_pointer(
        &surface,
        &pointer,
        None,
        zwp_pointer_constraints_v1::Lifetime::Persistent,
        &qh,
        (),
    );
    surface.commit();
    event_queue.roundtrip(&mut state)?;
    println!("SLOPOS_POINTER_CONFINE_REQUEST_ACCEPTED persistent=1");
    confined.destroy();
    event_queue.roundtrip(&mut state)?;

    constraints.destroy();
    pointer.release();
    surface.destroy();
    event_queue.roundtrip(&mut state)?;
    println!("SLOPOS_POINTER_CONSTRAINTS_OK");
    Ok(())
}
