// Copyright (c) 2026 Palaash Atri
// SPDX-License-Identifier: MIT

//! Repeatedly disconnect clients with live xdg-toplevel and xdg-popup roles.
//!
//! The process intentionally drops each Wayland connection without sending role
//! destroy requests. A compositor must release every per-client surface, popup,
//! focus and protocol record and continue accepting new clients.

use std::error::Error;

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_compositor, wl_registry, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};

const DEFAULT_CYCLES: u32 = 64;
const MAX_CYCLES: u32 = 1024;

#[derive(Clone, Copy)]
enum SurfaceRole {
    Toplevel,
    Popup,
}

#[derive(Default)]
struct State {
    toplevel_configured: bool,
    popup_configured: bool,
    popup_geometry_received: bool,
    close_requested: bool,
    popup_done: bool,
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
wayland_client::delegate_noop!(State: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(State: ignore xdg_positioner::XdgPositioner);

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

impl Dispatch<xdg_surface::XdgSurface, SurfaceRole> for State {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        role: &SurfaceRole,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            match role {
                SurfaceRole::Toplevel => state.toplevel_configured = true,
                SurfaceRole::Popup => state.popup_configured = true,
            }
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

impl Dispatch<xdg_popup::XdgPopup, ()> for State {
    fn event(
        state: &mut Self,
        _popup: &xdg_popup::XdgPopup,
        event: xdg_popup::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            xdg_popup::Event::Configure { width, height, .. } => {
                state.popup_geometry_received = width > 0 && height > 0;
            }
            xdg_popup::Event::PopupDone => state.popup_done = true,
            _ => {}
        }
    }
}

fn run_cycle(cycle: u32) -> Result<(), Box<dyn Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
    let handle = queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())?;
    let wm_base = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&handle, 1..=6, ())?;

    let parent_surface = compositor.create_surface(&handle, ());
    let parent_xdg = wm_base.get_xdg_surface(&parent_surface, &handle, SurfaceRole::Toplevel);
    let toplevel = parent_xdg.get_toplevel(&handle, ());
    toplevel.set_title(format!("SLOPOS disconnect stress {cycle}"));
    toplevel.set_app_id("io.github.palaashatri.slopos.disconnect-stress".to_owned());
    parent_surface.commit();

    let mut state = State::default();
    while !state.toplevel_configured && !state.close_requested {
        queue.blocking_dispatch(&mut state)?;
    }
    if state.close_requested {
        return Err(format!("cycle {cycle}: parent closed before configure").into());
    }
    parent_surface.commit();

    let positioner = wm_base.create_positioner(&handle, ());
    positioner.set_size(96, 48);
    positioner.set_anchor_rect(0, 0, 320, 24);
    let popup_surface = compositor.create_surface(&handle, ());
    let popup_xdg = wm_base.get_xdg_surface(&popup_surface, &handle, SurfaceRole::Popup);
    let _popup = popup_xdg.get_popup(Some(&parent_xdg), &positioner, &handle, ());
    popup_surface.commit();

    while !(state.close_requested
        || state.popup_done
        || (state.popup_configured && state.popup_geometry_received))
    {
        queue.blocking_dispatch(&mut state)?;
    }
    if state.close_requested || state.popup_done {
        return Err(format!("cycle {cycle}: popup dismissed before configure").into());
    }

    popup_surface.commit();
    connection.flush()?;

    // Intentionally send no destroy requests. Dropping Connection closes the
    // transport while all role proxies are live.
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let cycles = std::env::var("SLOPOS_DISCONNECT_STRESS_CYCLES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_CYCLES)
        .clamp(1, MAX_CYCLES);

    for cycle in 0..cycles {
        run_cycle(cycle)?;
    }

    println!("SLOPOS_ABRUPT_DISCONNECT_STRESS cycles={cycles}");
    Ok(())
}
