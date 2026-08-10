// Copyright (c) 2026 Palaash Atri
// SPDX-License-Identifier: MIT

//! Native two-client text-input-v3/input-method-v2 runtime probe.
//!
//! This example is intentionally a protocol client, not an IME implementation.
//! The `ime` mode binds `zwp_input_method_manager_v2` and sends one composed
//! update after the compositor reports the focused text input state. The `app`
//! mode owns a real XDG toplevel, enables `zwp_text_input_v3`, and records the
//! preedit, commit and delete events it receives. The shell gate runs the two
//! modes as separate Wayland clients so the compositor must actually connect
//! the input method to the focused application.

use std::{env, error::Error, io::Write, thread, time::Duration};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_compositor, wl_registry, wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3, zwp_text_input_v3,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2, zwp_input_method_v2,
};

#[derive(Default)]
struct AppState {
    toplevel_configured: bool,
    close_requested: bool,
    entered: bool,
    left: bool,
    done_count: u32,
    preedit: Option<(String, i32, i32)>,
    committed: Option<String>,
    deleted: Option<(u32, u32)>,
}

#[derive(Default)]
struct ImeState {
    activate_count: u32,
    deactivate_count: u32,
    active: bool,
    unavailable: bool,
    done_count: u32,
    surrounding: Option<(String, u32, u32)>,
    content_type: Option<(u32, u32)>,
    commit_sent: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for AppState {
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

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ImeState {
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

wayland_client::delegate_noop!(AppState: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(AppState: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(AppState: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(ImeState: ignore wl_seat::WlSeat);

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for AppState {
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

impl Dispatch<xdg_surface::XdgSurface, ()> for AppState {
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

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for AppState {
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

impl Dispatch<zwp_text_input_manager_v3::ZwpTextInputManagerV3, ()> for AppState {
    fn event(
        _state: &mut Self,
        _manager: &zwp_text_input_manager_v3::ZwpTextInputManagerV3,
        _event: zwp_text_input_manager_v3::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_text_input_v3::ZwpTextInputV3, ()> for AppState {
    fn event(
        state: &mut Self,
        _text_input: &zwp_text_input_v3::ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { .. } => {
                state.entered = true;
                println!("SLOPOS_TEXT_INPUT_APP_ENTER observed=true");
                let _ = std::io::stdout().flush();
            }
            zwp_text_input_v3::Event::Leave { .. } => state.left = true,
            zwp_text_input_v3::Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => state.preedit = Some((text.unwrap_or_default(), cursor_begin, cursor_end)),
            zwp_text_input_v3::Event::CommitString { text } => {
                state.committed = Some(text.unwrap_or_default())
            }
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => state.deleted = Some((before_length, after_length)),
            zwp_text_input_v3::Event::Done { .. } => {
                state.done_count = state.done_count.saturating_add(1);
                println!("SLOPOS_TEXT_INPUT_APP_DONE observed=true");
                let _ = std::io::stdout().flush();
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, ()> for ImeState {
    fn event(
        _state: &mut Self,
        _manager: &zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
        _event: zwp_input_method_manager_v2::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_input_method_v2::ZwpInputMethodV2, ()> for ImeState {
    fn event(
        state: &mut Self,
        input_method: &zwp_input_method_v2::ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _data: &(),
        connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_v2::Event::Activate => {
                state.activate_count = state.activate_count.saturating_add(1);
                state.active = true;
                println!("SLOPOS_IME_ACTIVATE observed=true");
                let _ = std::io::stdout().flush();
            }
            zwp_input_method_v2::Event::Deactivate => {
                state.deactivate_count = state.deactivate_count.saturating_add(1);
                state.active = false;
                println!("SLOPOS_IME_DEACTIVATE observed=true");
                let _ = std::io::stdout().flush();
            }
            zwp_input_method_v2::Event::SurroundingText {
                text,
                cursor,
                anchor,
            } => state.surrounding = Some((text, cursor, anchor)),
            zwp_input_method_v2::Event::ContentType { hint, purpose } => {
                state.content_type = Some((hint.into(), purpose.into()))
            }
            zwp_input_method_v2::Event::Done => {
                state.done_count = state.done_count.saturating_add(1);
                if state.active
                    && !state.commit_sent
                    && state.surrounding.is_some()
                    && state.content_type.is_some()
                {
                    input_method.commit_string("世界".to_owned());
                    input_method.set_preedit_string("かな".to_owned(), 0, 6);
                    input_method.delete_surrounding_text(1, 0);
                    input_method.commit(state.done_count);
                    if let Err(error) = connection.flush() {
                        eprintln!("SLOPOS_IME_COMMIT_FLUSH_FAILED error={error}");
                    } else {
                        state.commit_sent = true;
                        println!("SLOPOS_IME_COMMIT_SENT observed=true");
                        let _ = std::io::stdout().flush();
                    }
                }
            }
            zwp_input_method_v2::Event::Unavailable => {
                state.unavailable = true;
            }
            _ => {}
        }
    }
}

fn wait_for_app_toplevel(
    event_queue: &mut wayland_client::EventQueue<AppState>,
    state: &mut AppState,
) -> Result<(), Box<dyn Error>> {
    while !state.toplevel_configured {
        if state.close_requested {
            return Err("compositor closed the text-input app before configure".into());
        }
        event_queue.blocking_dispatch(state)?;
    }
    Ok(())
}

fn run_app(connection: &Connection) -> Result<(), Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init::<AppState>(connection)?;
    let queue_handle = event_queue.handle();
    let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())?;
    let wm_base = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&queue_handle, 1..=6, ())?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let text_manager = globals.bind::<zwp_text_input_manager_v3::ZwpTextInputManagerV3, _, _>(
        &queue_handle,
        1..=1,
        (),
    )?;
    let surface = compositor.create_surface(&queue_handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &queue_handle, ());
    let toplevel = xdg_surface.get_toplevel(&queue_handle, ());
    toplevel.set_title("SLOPOS text input probe".to_owned());
    toplevel.set_app_id("io.github.palaashatri.slopos.text-input-probe".to_owned());
    surface.commit();
    connection.flush()?;
    let mut state = AppState::default();
    wait_for_app_toplevel(&mut event_queue, &mut state)?;
    surface.commit();

    let text_input = text_manager.get_text_input(&seat, &queue_handle, ());
    text_input.enable();
    text_input.set_surrounding_text("café".to_owned(), 6, 6);
    text_input.set_text_change_cause(zwp_text_input_v3::ChangeCause::InputMethod);
    text_input.set_content_type(
        zwp_text_input_v3::ContentHint::None,
        zwp_text_input_v3::ContentPurpose::Normal,
    );
    text_input.set_cursor_rectangle(0, 0, 32, 20);
    text_input.commit();
    connection.flush()?;
    println!("SLOPOS_TEXT_INPUT_APP_READY observed=true");
    std::io::stdout().flush()?;

    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    while std::time::Instant::now() < deadline
        && !(state.entered
            && state.done_count >= 1
            && state.preedit.is_some()
            && state.committed.is_some()
            && state.deleted.is_some())
    {
        event_queue.blocking_dispatch(&mut state)?;
    }
    if !state.entered {
        return Err("text-input client never received enter".into());
    }
    if state.preedit.as_ref().map(|v| v.0.as_str()) != Some("かな") {
        return Err(format!("unexpected preedit: {:?}", state.preedit).into());
    }
    if state.committed.as_deref() != Some("世界") {
        return Err(format!("unexpected commit: {:?}", state.committed).into());
    }
    if state.deleted != Some((1, 0)) {
        return Err(format!("unexpected delete: {:?}", state.deleted).into());
    }
    println!("SLOPOS_TEXT_INPUT_PREEDIT_VERIFIED observed=true");
    println!("SLOPOS_TEXT_INPUT_COMMIT_VERIFIED observed=true");
    println!("SLOPOS_TEXT_INPUT_DELETE_VERIFIED observed=true");
    std::io::stdout().flush()?;
    text_input.disable();
    text_input.commit();
    connection.flush()?;
    thread::sleep(Duration::from_millis(250));
    Ok(())
}

fn run_ime(connection: &Connection) -> Result<(), Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init::<ImeState>(connection)?;
    let queue_handle = event_queue.handle();
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&queue_handle, 1..=9, ())?;
    let manager = globals.bind::<zwp_input_method_manager_v2::ZwpInputMethodManagerV2, _, _>(
        &queue_handle,
        1..=1,
        (),
    )?;
    let input_method = manager.get_input_method(&seat, &queue_handle, ());
    connection.flush()?;
    println!("SLOPOS_IME_READY observed=true");
    std::io::stdout().flush()?;
    let mut state = ImeState::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline
        && (!state.commit_sent || state.deactivate_count == 0)
    {
        event_queue.blocking_dispatch(&mut state)?;
    }
    if state.activate_count != 1 {
        return Err(format!(
            "expected exactly one input-method activate, got {}",
            state.activate_count
        )
        .into());
    }
    if state.surrounding.as_ref().map(|v| v.0.as_str()) != Some("café") {
        return Err(format!("unexpected surrounding text: {:?}", state.surrounding).into());
    }
    if state.content_type != Some((0, 0)) {
        return Err(format!("unexpected content type: {:?}", state.content_type).into());
    }
    if !state.commit_sent {
        return Err("input method did not send a serial-checked commit".into());
    }
    if state.deactivate_count != 1 {
        return Err(format!(
            "expected exactly one input-method deactivate, got {}",
            state.deactivate_count
        )
        .into());
    }
    println!("SLOPOS_TEXT_INPUT_SURROUNDING_VERIFIED observed=true");
    println!("SLOPOS_TEXT_INPUT_CONTENT_TYPE_VERIFIED observed=true");
    println!("SLOPOS_TEXT_INPUT_DEACTIVATE_VERIFIED observed=true");
    std::io::stdout().flush()?;
    drop(input_method);
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mode = env::args()
        .nth(1)
        .ok_or("usage: headless_text_input_client app|ime")?;
    let connection = Connection::connect_to_env()?;
    match mode.as_str() {
        "app" => run_app(&connection),
        "ime" => run_ime(&connection),
        _ => Err("mode must be app or ime".into()),
    }
}
