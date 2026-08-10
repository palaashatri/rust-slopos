//! slopos-compositor — minimal Wayland compositor using Smithay.
//!
//! This compositor replaces labwc in the SLOPOS-I stack. It:
//!   - Opens an X11 window (running nested under Xvfb on DISPLAY=:99)
//!   - Exposes a Wayland socket so slopos-shell (winit/wgpu) can connect
//!   - Implements xdg_shell, wl_shm, wl_seat for basic window management
//!   - Implements wl_data_device selection send (clipboard + primary store)
//!   - Optionally multi-output via SLOPOS_OUTPUTS=WxH,WxH or
//!     SLOPOS_OUTPUTS_LAYOUT (shell display arrange: name:WxH@x,y:sNN;...)
//!   - Optionally starts XWayland (best-effort under nested X11)
//!
//! Linux-only: requires libgbm, libdrm, libEGL, libxcb and libwayland-server.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("slopos-compositor is Linux-only (requires Wayland/DRM/GBM system libraries).");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    linux::run()
}

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::Context;
    use sha2::{Digest, Sha256};
    use std::collections::{HashMap, HashSet};
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::os::unix::io::OwnedFd;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use slopos_bus::{
        session_space_thumbnail_path, write_display_policy_snapshot, write_outputs_snapshot,
        write_space_thumbnail_manifest, write_spaces_snapshot, DisplayPolicyRequest,
        DisplayPolicySnapshot, HeadlessInputEvent, OutputSnapshot, OutputsSnapshot,
        SessionControlListener, SessionControlRequest, SpaceTargetWire, SpaceThumbnailEntry,
        SpaceThumbnailManifest, SpacesControlCommand, SpacesSnapshot, WindowPresentationAction,
    };
    use slopos_compositor::frame_timing::{FrameScheduler, RefreshRate};
    use slopos_compositor::hdr::{ColorSpace, HdrCapabilities, HdrFallbackReason};
    use slopos_compositor::work_area::{compute_exclusive_work_area, ExclusiveZoneReservation};
    use slopos_compositor::{
        accumulate_damage_for_window_move, accumulate_damage_rect, application_target_from_wire,
        application_target_to_wire, apply_scale_to_output_config, calculate_presentation_geometry,
        cascade_position, clamp_window_to_work_area, clear_interactive_grab_state,
        detect_output_scale_from_env, fullscreen_classification_from_wire,
        fullscreen_classification_to_wire, geometry_for_interactive_grab,
        intersecting_output_indices, move_to_top, multi_monitor_policy_from_wire,
        multi_monitor_policy_to_wire, new_session_epoch, next_cascade_offset, output_geometry,
        output_index_for_geometry, output_index_for_point, output_scale_summary,
        plan_window_output_migration, pointer_grab_request_is_valid_for_window, prefer_full_redraw,
        register_wayland_display_source, remap_geometry_between_outputs,
        resolve_laid_out_outputs_from_env, scale_logical_to_physical, scale_physical_to_logical,
        selection_bytes_for_mime_with_text_fallback, session_mode_note, surface_tree_root,
        text_input_capability_from_env, text_input_capability_summary, total_output_size,
        transition_presentation_state, validated_runtime_output_layout, window_paint_source,
        CompositorBackendKind, DamageRect, DisplayPolicy, InteractiveGrab, InteractiveGrabKind,
        LaidOutOutput, OutputScale, PlaceholderPresentStats, PointerConstraintMotion, ResizeEdges,
        SpaceId, SpaceTarget, SpacesError, SpacesModel, TextInputCapability, WindowGeometry,
        WindowPaintSource, WindowPresentationState, WindowRestoreState, WorkspaceId,
        WorkspaceState, WorkspaceSwipeAction, WorkspaceSwipeRecognizer, DEFAULT_WINDOW_H,
        DEFAULT_WINDOW_W,
    };
    use smithay::desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, utils::under_from_surface_tree,
        PopupGrab, PopupKeyboardGrab, PopupKind, PopupManager, PopupPointerGrab, WindowSurfaceType,
    };
    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
    use smithay::reexports::wayland_server::protocol::{
        wl_buffer, wl_data_source::WlDataSource, wl_seat,
    };
    use smithay::utils::Serial as WlSerial;
    use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
    use smithay::xwayland::xwm::{Reorder, ResizeEdge, X11Window, XwmId};
    use smithay::xwayland::{
        X11Surface as X11WmSurface, X11Wm, XWayland, XWaylandClientData, XWaylandEvent, XwmHandler,
    };
    use smithay::{
        backend::{
            allocator::{
                dmabuf::DmabufAllocator,
                gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            },
            egl::{EGLContext, EGLDisplay},
            input::{
                Axis, AxisRelativeDirection, AxisSource, ButtonState,
                InputEvent as BackendInputEvent, KeyboardKeyEvent, PointerAxisEvent,
                PointerButtonEvent, PointerMotionAbsoluteEvent,
            },
            renderer::{
                element::{
                    surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
                    Kind,
                },
                gles::GlesRenderer,
                utils::{draw_render_elements, on_commit_buffer_handler},
                Bind, Color32F, Frame, Renderer,
            },
            x11::{WindowBuilder, X11Backend, X11Event, X11Input, X11Surface},
        },
        delegate_compositor, delegate_foreign_toplevel_list, delegate_layer_shell, delegate_output,
        delegate_pointer_constraints, delegate_relative_pointer, delegate_seat, delegate_shm,
        delegate_xdg_shell,
        desktop::utils::send_frames_surface_tree,
        input::{
            keyboard::{FilterResult, KeyboardTarget, XkbConfig},
            pointer::{
                AxisFrame, ButtonEvent, CursorImageStatus, CursorImageSurfaceData, Focus,
                GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
                GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
                GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData, MotionEvent,
                PointerGrab, PointerHandle, PointerInnerHandle, RelativeMotionEvent,
            },
            Seat, SeatHandler, SeatState,
        },
        output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
        reexports::{
            calloop::{
                generic::Generic,
                timer::{TimeoutAction, Timer},
                EventLoop, Interest, LoopHandle, LoopSignal, Mode as CalloopMode, PostAction,
            },
            wayland_server::{
                backend::{ClientData, ClientId, DisconnectReason, GlobalId},
                protocol::{wl_output, wl_surface::WlSurface},
                Display, DisplayHandle, Resource,
            },
        },
        utils::{
            Clock, DeviceFd, Logical, Monotonic, Physical, Point, Rectangle, Serial, Size,
            Transform,
        },
        wayland::{
            buffer::BufferHandler,
            compositor::{with_states, CompositorClientState, CompositorHandler, CompositorState},
            foreign_toplevel_list::{
                ForeignToplevelHandle, ForeignToplevelListHandler, ForeignToplevelListState,
            },
            output::{OutputHandler, OutputManagerState},
            pointer_constraints::{
                with_pointer_constraint, PointerConstraint, PointerConstraintsHandler,
                PointerConstraintsState,
            },
            relative_pointer::RelativePointerManagerState,
            selection::{
                data_device::{
                    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler,
                    DataDeviceState, ServerDndGrabHandler,
                },
                primary_selection::{
                    set_primary_focus, PrimarySelectionHandler, PrimarySelectionState,
                },
                SelectionHandler, SelectionSource, SelectionTarget,
            },
            shell::wlr_layer::{
                Anchor, KeyboardInteractivity, Layer, LayerSurface, LayerSurfaceCachedState,
                Margins, WlrLayerShellHandler, WlrLayerShellState,
            },
            shell::xdg::{
                decoration::{XdgDecorationHandler, XdgDecorationState},
                PopupSurface, PositionerState, SurfaceCachedState, ToplevelSurface,
                XdgShellHandler, XdgShellState, XdgToplevelSurfaceData,
            },
            shm::{ShmHandler, ShmState},
            socket::ListeningSocketSource,
        },
    };
    use smithay::{delegate_primary_selection, delegate_xwayland_shell};

    // Retro gray: rgb(152, 152, 148) — the classic Mac OS desktop fill
    const RETRO_GRAY: (u8, u8, u8) = (152, 152, 148);
    const MAX_DISABLED_OUTPUT_GLOBALS: usize = 64;
    const XWAYLAND_RESTART_BUDGET: u8 = 3;
    const VIEWPORT_STATE_FILE: &str = "viewport-state.json";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) struct XWaylandRecoveryBudget {
        remaining: u8,
    }

    impl XWaylandRecoveryBudget {
        pub(super) const fn new(restarts: u8) -> Self {
            Self {
                remaining: restarts,
            }
        }

        pub(super) fn remaining(self) -> u8 {
            self.remaining
        }

        pub(super) fn take_restart(&mut self) -> bool {
            if self.remaining == 0 {
                return false;
            }

            self.remaining -= 1;
            true
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct X11SceneEntryState {
        geometry: Rectangle<i32, Logical>,
        mapped: bool,
        associated: bool,
    }

    /// Stable compositor-owned Spaces membership key for a rootless X11
    /// window.  X11 windows do not expose a Wayland foreign-toplevel ID, so
    /// the X11 protocol ID is namespaced to avoid colliding with native IDs.
    fn x11_space_window_id(window_id: X11Window) -> String {
        format!("x11:{window_id}")
    }

    fn x11_window_visible_on_space(
        spaces: &SpacesModel,
        window_id: X11Window,
        space: SpaceId,
    ) -> bool {
        spaces
            .window_spaces(&x11_space_window_id(window_id))
            .contains(&space)
    }

    impl X11SceneEntryState {
        fn new(geometry: Rectangle<i32, Logical>) -> Self {
            Self {
                geometry,
                mapped: false,
                associated: false,
            }
        }

        fn visible(self) -> bool {
            self.mapped && self.associated
        }

        fn set_associated(&mut self, associated: bool) {
            self.associated = associated;
        }

        fn set_mapped(&mut self, mapped: bool) {
            self.mapped = mapped;
        }

        fn set_geometry(&mut self, geometry: Rectangle<i32, Logical>) {
            self.geometry = geometry;
        }
    }

    #[derive(Clone)]
    struct X11SceneEntry {
        surface: X11WmSurface,
        wl_surface: Option<WlSurface>,
        state: X11SceneEntryState,
        mapped_marker_emitted: bool,
        rendered_marker_emitted: bool,
    }

    #[derive(Default)]
    struct X11SceneRegistry {
        /// Entries are kept bottom-to-top, matching XWM discovery order.
        entries: Vec<X11SceneEntry>,
    }

    impl X11SceneRegistry {
        fn index_for(&self, window_id: X11Window) -> Option<usize> {
            self.entries
                .iter()
                .position(|entry| entry.surface.window_id() == window_id)
        }

        fn register(&mut self, surface: X11WmSurface) {
            let window_id = surface.window_id();
            let geometry = surface.geometry();
            if let Some(index) = self.index_for(window_id) {
                let entry = &mut self.entries[index];
                entry.surface = surface;
                entry.state.set_geometry(geometry);
                return;
            }

            self.entries.push(X11SceneEntry {
                surface,
                wl_surface: None,
                state: X11SceneEntryState::new(geometry),
                mapped_marker_emitted: false,
                rendered_marker_emitted: false,
            });
        }

        fn associate(&mut self, surface: X11WmSurface, wl_surface: WlSurface) {
            let window_id = surface.window_id();
            self.register(surface);
            if let Some(index) = self.index_for(window_id) {
                let entry = &mut self.entries[index];
                entry.wl_surface = Some(wl_surface);
                entry.state.set_associated(true);
                entry.rendered_marker_emitted = false;
            }
        }

        fn set_mapped(&mut self, surface: X11WmSurface, mapped: bool) {
            let window_id = surface.window_id();
            self.register(surface);
            if let Some(index) = self.index_for(window_id) {
                let entry = &mut self.entries[index];
                entry.state.set_mapped(mapped);
                if !mapped {
                    entry.mapped_marker_emitted = false;
                    entry.rendered_marker_emitted = false;
                }
            }
        }

        fn configure(&mut self, surface: X11WmSurface, geometry: Rectangle<i32, Logical>) {
            let window_id = surface.window_id();
            self.register(surface);
            if let Some(index) = self.index_for(window_id) {
                self.entries[index].state.set_geometry(geometry);
            }
        }

        fn unmap(&mut self, window_id: X11Window) {
            if let Some(index) = self.index_for(window_id) {
                self.entries[index].state.set_mapped(false);
                self.entries[index].mapped_marker_emitted = false;
                self.entries[index].rendered_marker_emitted = false;
            }
        }

        fn destroy(&mut self, window_id: X11Window) -> Option<X11SceneEntry> {
            let index = self.index_for(window_id)?;
            Some(self.entries.remove(index))
        }

        fn clear(&mut self) -> Vec<X11SceneEntry> {
            std::mem::take(&mut self.entries)
        }

        fn associated_surface(&self, window_id: X11Window) -> Option<WlSurface> {
            self.entries
                .iter()
                .find(|entry| entry.surface.window_id() == window_id)
                .and_then(|entry| entry.wl_surface.clone())
        }

        fn window_for_surface(&self, surface: &WlSurface) -> Option<X11WmSurface> {
            self.entries
                .iter()
                .find(|entry| entry.wl_surface.as_ref() == Some(surface))
                .map(|entry| entry.surface.clone())
        }

        fn x11_surface_accepts_keyboard_focus(&self, window_id: X11Window) -> bool {
            self.entries
                .iter()
                .find(|entry| entry.surface.window_id() == window_id)
                .is_some_and(|entry| !entry.surface.is_override_redirect())
        }

        fn set_active(&self, active_window: Option<X11Window>) {
            for entry in &self.entries {
                let active = Some(entry.surface.window_id()) == active_window;
                let _ = entry.surface.set_activated(active);
            }
        }

        fn geometry(&self, window_id: X11Window) -> Option<Rectangle<i32, Logical>> {
            self.entries
                .iter()
                .find(|entry| entry.surface.window_id() == window_id)
                .map(|entry| entry.state.geometry)
        }

        /// Return a mapped, associated X11 surface that is still alive.
        ///
        /// Focus can outlive an XWayland map/unmap notification by one event
        /// loop turn.  Callers that mutate scene geometry must therefore
        /// revalidate the registry entry instead of trusting the cached focus
        /// handle alone.
        fn visible_surface(&self, window_id: X11Window) -> Option<X11WmSurface> {
            self.entries
                .iter()
                .find(|entry| {
                    entry.surface.window_id() == window_id
                        && entry.state.visible()
                        && entry.surface.alive()
                })
                .map(|entry| entry.surface.clone())
        }

        fn associated_targets(
            &self,
            spaces: &SpacesModel,
        ) -> Vec<(X11Window, WlSurface, Rectangle<i32, Logical>)> {
            self.associated_targets_on_space(spaces, spaces.active_space())
        }

        fn associated_targets_on_space(
            &self,
            spaces: &SpacesModel,
            space: SpaceId,
        ) -> Vec<(X11Window, WlSurface, Rectangle<i32, Logical>)> {
            self.entries
                .iter()
                .filter(|entry| {
                    entry.state.visible()
                        && entry.surface.alive()
                        && x11_window_visible_on_space(spaces, entry.surface.window_id(), space)
                })
                .filter_map(|entry| {
                    entry
                        .wl_surface
                        .clone()
                        .map(|surface| (entry.surface.window_id(), surface, entry.state.geometry))
                })
                .collect()
        }

        fn window_ids(&self) -> impl Iterator<Item = String> + '_ {
            self.entries
                .iter()
                .map(|entry| x11_space_window_id(entry.surface.window_id()))
        }

        fn associated_surfaces(&self) -> Vec<(X11Window, WlSurface, Rectangle<i32, Logical>)> {
            self.entries
                .iter()
                .filter(|entry| entry.surface.alive())
                .filter_map(|entry| {
                    entry
                        .wl_surface
                        .clone()
                        .map(|surface| (entry.surface.window_id(), surface, entry.state.geometry))
                })
                .collect()
        }

        fn take_mapped_marker(&mut self, window_id: X11Window) -> bool {
            let Some(index) = self.index_for(window_id) else {
                return false;
            };
            let entry = &mut self.entries[index];
            if !entry.state.visible() || entry.mapped_marker_emitted {
                return false;
            }
            entry.mapped_marker_emitted = true;
            true
        }

        fn take_rendered_marker(&mut self, window_id: X11Window) -> bool {
            let Some(index) = self.index_for(window_id) else {
                return false;
            };
            let entry = &mut self.entries[index];
            if !entry.state.visible() || entry.rendered_marker_emitted {
                return false;
            }
            entry.rendered_marker_emitted = true;
            true
        }
    }

    fn spaces_persistence_path() -> Option<PathBuf> {
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".local/share"))
            })?;
        Some(data_home.join("slopos-i").join("spaces.json"))
    }

    fn load_initial_spaces_model() -> SpacesModel {
        if let Some(path) = spaces_persistence_path() {
            match SpacesModel::load(&path) {
                Ok(mut model) => {
                    model.clear_window_memberships();
                    return model;
                }
                Err(error) if path.exists() => match SpacesModel::quarantine_invalid_state(&path) {
                    Ok(Some(quarantined)) => tracing::warn!(
                        %error,
                        path = %path.display(),
                        quarantine = %quarantined.display(),
                        "quarantined invalid persisted Spaces model"
                    ),
                    Ok(None) => tracing::warn!(
                        %error,
                        path = %path.display(),
                        "invalid persisted Spaces model disappeared before quarantine"
                    ),
                    Err(quarantine_error) => tracing::error!(
                        %error,
                        %quarantine_error,
                        path = %path.display(),
                        "could not quarantine invalid persisted Spaces model"
                    ),
                },
                Err(_) => {}
            }
        }
        SpacesModel::with_default_count(8).expect("default Spaces model is valid")
    }

    // Window placeholder colors (cycling palette for distinguishing windows)
    const WIN_COLORS: &[(f32, f32, f32)] = &[
        (0.502, 0.502, 1.000), // soft blue
        (0.502, 1.000, 0.502), // soft green
        (1.000, 0.502, 0.502), // soft red
        (1.000, 1.000, 0.502), // soft yellow
        (0.502, 1.000, 1.000), // soft cyan
        (1.000, 0.502, 1.000), // soft magenta
    ];

    /// Compositor-owned selection payload keyed by mime type.
    /// Used as [`SelectionHandler::SelectionUserData`] for server-set selections.
    type MimePayload = Arc<HashMap<String, Vec<u8>>>;

    // -----------------------------------------------------------------------
    // Per-client data
    // -----------------------------------------------------------------------

    #[derive(Default)]
    struct ClientState {
        compositor_state: CompositorClientState,
    }

    impl ClientData for ClientState {
        fn initialized(&self, _client_id: ClientId) {
            eprintln!("[slopos-compositor] client connected");
        }
        fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {
            eprintln!("[slopos-compositor] client disconnected");
        }
    }

    // -----------------------------------------------------------------------
    // Tracked surface: a mapped xdg_toplevel with a compositor-space position
    // -----------------------------------------------------------------------

    #[derive(Clone)]
    struct MappedWindow {
        toplevel: ToplevelSurface,
        /// Foreign-toplevel-list handle for task list / Force Quit / overview.
        foreign: ForeignToplevelHandle,
        /// Stable id for workspace visibility (`foreign.identifier()` at map).
        window_id: String,
        /// Wayland app_id captured at map time; the shell uses compositor focus
        /// state to select the corresponding global-menu manifest.
        app_id: String,
        /// Top-left position in logical compositor space
        position: Point<i32, Logical>,
        /// Last committed size (logical pixels)
        size: Size<i32, Logical>,
        /// Single-authority presentation state (Normal, Minimized, SmartZoomed, Filled, Fullscreen, Tiled).
        presentation_state: WindowPresentationState,
        /// Saved restore state prior to zoom/fill/fullscreen/tiling.
        restore_state: Option<WindowRestoreState>,
        /// Minimized windows stay mapped but are excluded from hit-testing/painting.
        minimized: bool,
    }

    struct MappedLayer {
        surface: LayerSurface,
        layer: Layer,
        namespace: String,
        /// Exact logical output selected by the layer-shell request.
        output_index: usize,
        /// Authoritative compositor-space placement of the layer surface.
        geo: Rectangle<i32, Logical>,
        /// Exclusive work-area reservation requested by the layer client.
        exclusive_zone: i32,
        /// Last client-requested logical size.  Zero means compositor-sized.
        requested: Size<i32, Logical>,
        /// Last configure serial emitted by the compositor for this surface.
        configure_serial: u32,
        /// A matching committed buffer is the observable acknowledgement point
        /// for the configure serial.  Smithay validates the protocol ack before
        /// exposing the new current state; retaining the serial here makes that
        /// state available to the runtime viewport evidence.
        ack_serial: Option<u32>,
        /// Whether this layer has committed at least one buffer.
        has_committed: bool,
        /// Frame revision at which the last committed buffer was presented.
        committed_frame_revision: u64,
    }

    #[derive(Clone)]
    struct PointerPress {
        serial: Serial,
        /// Mapped toplevel that owns the hit toplevel/popup surface tree.
        window_id: String,
        /// XWayland window that owns the hit scene surface, when the press was
        /// delivered to a rootless X11 client rather than a native XDG client.
        x11_window_id: Option<X11Window>,
    }

    #[derive(Clone)]
    struct X11InteractiveGrab {
        window_id: X11Window,
        surface: X11WmSurface,
        policy: InteractiveGrab,
    }

    struct InteractivePointerGrab {
        start_data: GrabStartData<SloposCompositor>,
    }

    impl PointerGrab<SloposCompositor> for InteractivePointerGrab {
        fn motion(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            _focus: Option<(WlSurface, Point<f64, Logical>)>,
            event: &MotionEvent,
        ) {
            if !data.update_interactive_grab() {
                handle.unset_grab(self, data, event.serial, event.time, true);
                return;
            }
            handle.motion(data, self.start_data.focus.clone(), event);
        }

        fn relative_motion(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            _focus: Option<(WlSurface, Point<f64, Logical>)>,
            event: &RelativeMotionEvent,
        ) {
            if !data.update_interactive_grab() {
                let serial = data.next_serial();
                let time = u32::try_from(event.utime / 1_000)
                    .unwrap_or(u32::MAX)
                    .max(1);
                handle.unset_grab(self, data, serial, time, true);
                return;
            }
            handle.relative_motion(data, self.start_data.focus.clone(), event);
        }

        fn button(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &ButtonEvent,
        ) {
            handle.button(data, event);
            if event.state == ButtonState::Released && handle.current_pressed().is_empty() {
                handle.unset_grab(self, data, event.serial, event.time, true);
            }
        }

        fn axis(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            details: AxisFrame,
        ) {
            handle.axis(data, details);
        }

        fn frame(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
        ) {
            handle.frame(data);
        }

        fn gesture_swipe_begin(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &GestureSwipeBeginEvent,
        ) {
            handle.gesture_swipe_begin(data, event);
        }

        fn gesture_swipe_update(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &GestureSwipeUpdateEvent,
        ) {
            handle.gesture_swipe_update(data, event);
        }

        fn gesture_swipe_end(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &GestureSwipeEndEvent,
        ) {
            handle.gesture_swipe_end(data, event);
        }

        fn gesture_pinch_begin(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &GesturePinchBeginEvent,
        ) {
            handle.gesture_pinch_begin(data, event);
        }

        fn gesture_pinch_update(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &GesturePinchUpdateEvent,
        ) {
            handle.gesture_pinch_update(data, event);
        }

        fn gesture_pinch_end(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &GesturePinchEndEvent,
        ) {
            handle.gesture_pinch_end(data, event);
        }

        fn gesture_hold_begin(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &GestureHoldBeginEvent,
        ) {
            handle.gesture_hold_begin(data, event);
        }

        fn gesture_hold_end(
            &mut self,
            data: &mut SloposCompositor,
            handle: &mut PointerInnerHandle<'_, SloposCompositor>,
            event: &GestureHoldEndEvent,
        ) {
            handle.gesture_hold_end(data, event);
        }

        fn start_data(&self) -> &GrabStartData<SloposCompositor> {
            &self.start_data
        }

        fn unset(&mut self, data: &mut SloposCompositor) {
            data.finish_interactive_grab();
        }
    }

    fn layer_policy_defaults(
        namespace: &str,
        output: Size<i32, Logical>,
    ) -> (Size<i32, Logical>, Anchor) {
        match namespace {
            "slopos-i-menu" | "menu-bar" => (
                Size::from((output.w, 24)),
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            ),
            "slopos-i-dock" | "dock" => (
                Size::from((output.w, 64)),
                Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            ),
            "slopos-i-menu-popup" => (Size::from((1, 1)), Anchor::TOP | Anchor::LEFT),
            _ => (
                output,
                Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            ),
        }
    }

    fn layer_geometry_for(
        namespace: &str,
        layer: Layer,
        output: Size<i32, Logical>,
        requested: Size<i32, Logical>,
        anchor: Anchor,
        margins: Margins,
    ) -> Rectangle<i32, Logical> {
        let (fallback_size, fallback_anchor) = layer_policy_defaults(namespace, output);
        let anchor = if anchor.is_empty() {
            fallback_anchor
        } else {
            anchor
        };
        let left = margins.left.max(0);
        let right = margins.right.max(0);
        let top = margins.top.max(0);
        let bottom = margins.bottom.max(0);

        let width = if requested.w == 0 {
            if anchor.anchored_horizontally() {
                (output.w - left - right).max(1)
            } else {
                fallback_size.w
            }
        } else {
            requested.w
        }
        .clamp(1, output.w.max(1));
        let height = if requested.h == 0 {
            if anchor.anchored_vertically() {
                (output.h - top - bottom).max(1)
            } else {
                fallback_size.h
            }
        } else {
            requested.h
        }
        .clamp(1, output.h.max(1));

        let x = if anchor.contains(Anchor::LEFT) && anchor.contains(Anchor::RIGHT) {
            left
        } else if anchor.contains(Anchor::RIGHT) {
            (output.w - width - right).max(0)
        } else if anchor.contains(Anchor::LEFT) {
            left
        } else {
            (output.w - width) / 2
        };
        let y = if anchor.contains(Anchor::TOP) && anchor.contains(Anchor::BOTTOM) {
            top
        } else if anchor.contains(Anchor::BOTTOM) {
            (output.h - height - bottom).max(0)
        } else if anchor.contains(Anchor::TOP) {
            top
        } else {
            (output.h - height) / 2
        };

        // `layer` is intentionally part of the policy signature: layer order
        // controls composition, while anchors control geometry. Keeping both
        // here prevents callers from accidentally treating a Bottom surface as
        // a normal xdg window when adding new chrome roles.
        let _ = layer;
        Rectangle::new((x, y).into(), (width, height).into())
    }

    fn layer_surface_request(surface: &LayerSurface) -> (Size<i32, Logical>, Anchor, Margins, i32) {
        with_states(surface.wl_surface(), |states| {
            let mut cached = states.cached_state.get::<LayerSurfaceCachedState>();
            let current = *cached.current();
            (
                current.size,
                current.anchor,
                current.margin,
                current.exclusive_zone.into(),
            )
        })
    }

    /// Convert a surface-tree hit origin (relative to the layer surface) into
    /// compositor-space logical coordinates.
    pub(super) fn layer_surface_hit_origin(
        layer_origin: Point<i32, Logical>,
        surface_origin: Point<i32, Logical>,
    ) -> Point<f64, Logical> {
        Point::from((
            layer_origin.x as f64 + surface_origin.x as f64,
            layer_origin.y as f64 + surface_origin.y as f64,
        ))
    }

    impl MappedWindow {
        fn geometry(&self) -> WindowGeometry {
            WindowGeometry::new(self.position.x, self.position.y, self.size.w, self.size.h)
        }
    }

    pub(super) fn x11_resize_edge_to_resize_edges(edge: ResizeEdge) -> ResizeEdges {
        match edge {
            ResizeEdge::Top => ResizeEdges::TOP,
            ResizeEdge::Bottom => ResizeEdges::BOTTOM,
            ResizeEdge::Left => ResizeEdges::LEFT,
            ResizeEdge::Right => ResizeEdges::RIGHT,
            ResizeEdge::TopLeft => ResizeEdges::TOP_LEFT,
            ResizeEdge::BottomLeft => ResizeEdges::BOTTOM_LEFT,
            ResizeEdge::TopRight => ResizeEdges::TOP_RIGHT,
            ResizeEdge::BottomRight => ResizeEdges::BOTTOM_RIGHT,
        }
    }

    // -----------------------------------------------------------------------
    // Main compositor state
    // -----------------------------------------------------------------------

    struct SloposCompositor {
        display_handle: DisplayHandle,
        _loop_signal: LoopSignal,
        loop_handle: LoopHandle<'static, SloposCompositor>,
        clock: Clock<Monotonic>,

        // Smithay protocol states
        compositor_state: CompositorState,
        shm_state: ShmState,
        seat_state: SeatState<SloposCompositor>,
        _relative_pointer_state: RelativePointerManagerState,
        _pointer_constraints_state: PointerConstraintsState,
        xdg_shell_state: XdgShellState,
        data_device_state: DataDeviceState,
        primary_selection_state: PrimarySelectionState,
        _output_manager_state: OutputManagerState,
        xwayland_shell_state: XWaylandShellState,
        layer_shell_state: WlrLayerShellState,
        foreign_toplevel_list: ForeignToplevelListState,
        _xdg_decoration_state: XdgDecorationState,
        /// Present when SLOPOS_TEXT_INPUT enables text-input-v3 global.
        _text_input_state: Option<smithay::wayland::text_input::TextInputManagerState>,
        /// Present when SLOPOS_TEXT_INPUT=full|im enables input-method-v2.
        _input_method_state: Option<smithay::wayland::input_method::InputMethodManagerState>,
        /// Input-method popup surfaces (IME UI).
        im_popups: Vec<smithay::wayland::input_method::PopupSurface>,

        seat: Seat<SloposCompositor>,
        /// Registered wl_output objects (one or more; multi-output via SLOPOS_OUTPUTS).
        /// Kept alive so globals stay registered for the compositor lifetime.
        outputs: Vec<Output>,
        /// Global ids parallel to `outputs`, retained so runtime hotplug can disable them.
        output_globals: Vec<GlobalId>,
        /// Disabled globals remain alive for existing clients for this session.
        disabled_output_globals: Vec<GlobalId>,
        /// Normalized logical output rectangles used for window assignment.
        laid_out_outputs: Vec<LaidOutOutput>,
        /// Connector or synthetic names parallel to `laid_out_outputs`.
        output_names: Vec<String>,
        output_scale: OutputScale,
        refresh_mhz: i32,
        backend_kind: CompositorBackendKind,
        outputs_revision: u64,
        running: bool,

        // Mapped windows (in painting order, bottom → top)
        windows: Vec<MappedWindow>,
        /// Virtual workspaces: only active-workspace windows are painted.
        workspace_state: WorkspaceState,
        /// Dynamic compositor-authoritative Spaces metadata and membership.
        spaces: SpacesModel,
        /// Session epoch used to make shell revision reconciliation restart-safe.
        spaces_session_epoch: u64,
        spaces_revision: u64,
        // Layer-shell chrome (menu bar, dock, notifications, …)
        layer_surfaces: Vec<MappedLayer>,
        /// Tracks xdg popup trees independently of ordinary toplevel windows.
        popup_manager: PopupManager,
        /// The currently active popup grab, if a client requested one.
        popup_grab: Option<PopupGrab<SloposCompositor>>,
        /// Window whose xdg_toplevel state currently carries Activated.
        activated_window_id: Option<String>,
        /// Generic Restore targets the most recently minimized client. Focus
        /// moves to another visible window after minimize, so the active id
        /// alone cannot identify the Dock restore target.
        last_minimized_window_id: Option<String>,
        // Counter for cascading new window placement
        next_window_offset: i32,
        // Current compositor-visible pointer position (logical).
        pointer_pos: Point<f64, Logical>,
        /// Last raw absolute sample from the nested X11 backend. Kept separate
        /// so relative-pointer deltas continue while an app locks the visible cursor.
        last_backend_pointer_pos: Option<Point<f64, Logical>>,
        /// Client requested cursor surface/name; Named always has a software fallback.
        cursor_status: CursorImageStatus,
        /// Test-only pointer injection is enabled only for the explicit
        /// headless backend with SLOPOS_TEST_INPUT=1.
        headless_test_input_enabled: bool,
        /// Current compositor-owned xdg_toplevel move/resize operation.
        interactive_grab: Option<InteractiveGrab>,
        /// Current compositor-owned rootless XWayland move/resize operation.
        x11_interactive_grab: Option<X11InteractiveGrab>,
        /// Reducer for explicitly injected headless three-finger gestures.
        workspace_swipe: WorkspaceSwipeRecognizer,
        /// Set by the typed Spaces thumbnail request and consumed on the next
        /// renderer-backed frame. Headless sessions intentionally leave the
        /// generated files absent because they have no real pixels to capture.
        thumbnail_refresh_requested: bool,
        /// Tracks BTN_LEFT so stale xdg move/resize requests cannot start a grab.
        left_button_down: bool,
        /// The most recent left-button press delivered to an application surface.
        /// xdg_toplevel.move/resize must consume this exact serial while held.
        last_pointer_press: Option<PointerPress>,
        /// A frame is produced only after damage, input, commit, or a frame event.
        frame_dirty: bool,
        // Output size advertised for X11 input transforms (union of all outputs).
        output_size: Size<i32, Physical>,
        // Serial counter for synthetic events
        serial: u32,
        /// Monotonic revision of compositor frames used by viewport evidence.
        viewport_frame_revision: u64,

        // GL rendering
        renderer: Option<GlesRenderer>,
        x11_surface: Option<X11Surface>,

        // ---- selection / DnD store (P1.1) ----
        /// Last client clipboard SelectionSource (for tracking / XWayland bridge).
        clipboard_source: Option<SelectionSource>,
        /// Last client primary SelectionSource.
        primary_source: Option<SelectionSource>,
        /// Compositor-owned clipboard mime → bytes (server-set selections).
        clipboard_data: HashMap<String, Vec<u8>>,
        /// Compositor-owned primary mime → bytes.
        primary_data: HashMap<String, Vec<u8>>,
        /// Server-initiated DnD mime payloads (written in ServerDndGrabHandler::send).
        server_dnd_data: HashMap<String, Vec<u8>>,
        /// Client DnD icon surface (if any).
        dnd_icon: Option<WlSurface>,

        // ---- HDR / VRR (P1.4) ----
        /// Applied policy snapshot (logged at startup; retained for introspection).
        #[allow(dead_code)]
        display_policy: DisplayPolicy,
        #[allow(dead_code)]
        hdr_caps: HdrCapabilities,
        frame_scheduler: FrameScheduler,
        /// Whether this backend has a real runtime VRR transaction path.
        vrr_supported: bool,
        /// Monotonic revision for the authoritative display-policy projection.
        display_policy_revision: u64,
        /// Last applied HDR/colour fallback, if policy was not exact.
        display_policy_fallback_reason: Option<String>,

        // ---- Damage / present honesty ----
        /// Union of dirty regions from window moves/resizes (partial present plan).
        pending_damage: Option<DamageRect>,
        /// Set on workspace switch so the next frame redraws the full output.
        need_full_redraw: bool,
        /// Counts frames that fell back to solid placeholders; logs once per session.
        placeholder_stats: PlaceholderPresentStats,

        // ---- XWayland (P1.3) ----
        xwm: Option<X11Wm>,
        xdisplay: Option<u32>,
        /// Wayland client identity for the current XWayland process. It lets
        /// the compositor detect displayfd EOF before Smithay emits Ready.
        xwayland_client_id: Option<ClientId>,
        /// The startup watchdog is session-scoped and registered once.
        xwayland_startup_watchdog_started: bool,
        /// Session-scoped cap on XWayland WM recovery attempts.
        xwayland_recovery_budget: XWaylandRecoveryBudget,
        /// X11 surfaces associated with Wayland trees and their scene state.
        x11_scene: X11SceneRegistry,
        /// X11 keyboard target whose X11 input focus was explicitly selected.
        xwayland_keyboard_focus: Option<X11WmSurface>,
        /// Wayland socket name advertised to spawned clients (Super+O/L shortcuts).
        wayland_socket_name: String,
    }

    pub(super) fn bind_session_control_listener(
        runtime: &std::path::Path,
    ) -> anyhow::Result<SessionControlListener> {
        SessionControlListener::bind(runtime)
            .map_err(|error| anyhow::anyhow!("bind session control socket: {error}"))
    }

    impl SloposCompositor {
        /// Allocate the next serial (wrapping)
        fn next_serial(&mut self) -> Serial {
            self.serial = self.serial.wrapping_add(1);
            Serial::from(self.serial)
        }

        fn spaces_snapshot(&self) -> SpacesSnapshot {
            SpacesSnapshot {
                session_epoch: self.spaces_session_epoch,
                revision: self.spaces_revision,
                active_space: self.spaces.active_space().get(),
                multi_monitor_policy: multi_monitor_policy_to_wire(
                    self.spaces.multi_monitor_policy(),
                ),
                application_policies: self
                    .spaces
                    .application_policies()
                    .iter()
                    .map(
                        |(app_id, target)| slopos_bus::ApplicationSpacePolicySnapshot {
                            app_id: app_id.clone(),
                            target: application_target_to_wire(target),
                        },
                    )
                    .collect(),
                spaces: self
                    .spaces
                    .overview()
                    .into_iter()
                    .map(|space| slopos_bus::SpaceSnapshot {
                        id: space.id().get(),
                        order: space.order(),
                        name: space.name().to_owned(),
                        active: space.active(),
                        window_count: space.window_count(),
                        wallpaper: space.wallpaper().map(str::to_owned),
                        appearance: space.appearance().map(str::to_owned),
                        classification: fullscreen_classification_to_wire(space.classification()),
                        output_id: space.output_id().map(str::to_owned),
                    })
                    .collect(),
            }
        }

        /// Keep legacy indexed helpers available for older in-process paths.
        /// Dynamic visibility and membership always consult `self.spaces`.
        fn sync_legacy_workspace_state(&mut self) {
            let mut mirror = WorkspaceState::new();
            if let Ok(active) = u8::try_from(self.spaces.active_index()) {
                if let Some(active) = WorkspaceId::new(active) {
                    let _ = mirror.activate(active);
                }
            }
            for (order, space) in self.spaces.spaces().iter().enumerate().take(8) {
                let Ok(index) = u8::try_from(order) else {
                    break;
                };
                let Some(workspace) = WorkspaceId::new(index) else {
                    break;
                };
                for window_id in space.windows() {
                    let _ = mirror.assign_window(window_id.clone(), workspace);
                }
            }
            self.workspace_state = mirror;
        }

        fn window_visible_on_active(&self, window_id: &str) -> bool {
            self.window_visible_on_space(window_id, self.spaces.active_space())
        }

        fn window_visible_on_space(&self, window_id: &str, space: SpaceId) -> bool {
            self.spaces
                .window_spaces(window_id)
                .into_iter()
                .any(|candidate| candidate == space)
        }

        fn publish_spaces_state(&mut self, persist: bool) {
            self.spaces_revision = self.spaces_revision.saturating_add(1);
            let snapshot = self.spaces_snapshot();
            if let Err(error) = write_spaces_snapshot(&snapshot) {
                tracing::debug!(%error, "could not publish Spaces snapshot");
            }
            if persist {
                if let Some(path) = spaces_persistence_path() {
                    if let Some(parent) = path.parent() {
                        if let Err(error) = std::fs::create_dir_all(parent) {
                            tracing::warn!(%error, path = %parent.display(), "could not create Spaces data directory");
                        } else if let Err(error) = self.spaces.save_atomic(&path) {
                            tracing::warn!(%error, path = %path.display(), "could not persist Spaces model");
                        }
                    }
                }
            }
        }

        fn publish_outputs_state(&mut self) {
            self.outputs_revision = self.outputs_revision.saturating_add(1);
            let scale_percent = self.runtime_scale_percent();
            let snapshot = OutputsSnapshot {
                backend: match self.backend_kind {
                    CompositorBackendKind::NestedX11 => "nested".to_string(),
                    CompositorBackendKind::Headless => "headless".to_string(),
                    CompositorBackendKind::SessionDrm => "drm".to_string(),
                },
                revision: self.outputs_revision,
                outputs: self
                    .laid_out_outputs
                    .iter()
                    .enumerate()
                    .map(|(index, output)| OutputSnapshot {
                        name: self
                            .output_names
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("output-{index}")),
                        width: output.config.width.max(1) as u32,
                        height: output.config.height.max(1) as u32,
                        x: output.x,
                        y: output.y,
                        scale_percent,
                        primary: index == 0,
                    })
                    .collect(),
            };
            if let Err(error) = write_outputs_snapshot(&snapshot) {
                tracing::debug!(%error, "could not publish output topology snapshot");
            }
        }

        fn configure_layer(layer: &mut MappedLayer) {
            let serial = layer.surface.send_configure();
            layer.configure_serial = u32::from(serial);
            layer.ack_serial = None;
            layer.has_committed = false;
            layer.committed_frame_revision = 0;
        }

        fn viewport_output_name(&self) -> &str {
            self.output_names
                .first()
                .map(String::as_str)
                .unwrap_or("output-0")
        }

        fn publish_viewport_state(&self, framebuffer: &std::path::Path) -> anyhow::Result<()> {
            let runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("SLOPOS_SESSION_RUNTIME_DIR is not set"))?;
            let output = self
                .laid_out_outputs
                .first()
                .ok_or_else(|| anyhow::anyhow!("viewport state has no output"))?;
            let logical_width = output.config.width.max(1) as u32;
            let logical_height = output.config.height.max(1) as u32;
            let physical_width = self.output_size.w.max(1) as u32;
            let physical_height = self.output_size.h.max(1) as u32;
            let requested_scale = self.output_scale.reduced();
            let effective_scale = Self::effective_nested_output_scale(requested_scale);
            let expected_physical = apply_scale_to_output_config(
                slopos_compositor::OutputConfig {
                    width: logical_width as i32,
                    height: logical_height as i32,
                },
                effective_scale,
            );
            if expected_physical.width.max(1) as u32 != physical_width
                || expected_physical.height.max(1) as u32 != physical_height
            {
                return Err(anyhow::anyhow!(
                    "viewport physical extent {}x{} disagrees with logical {}x{} at effective scale {}/{} (expected {}x{})",
                    physical_width,
                    physical_height,
                    logical_width,
                    logical_height,
                    effective_scale.numerator,
                    effective_scale.denominator,
                    expected_physical.width,
                    expected_physical.height,
                ));
            }
            let mut hasher = Sha256::new();
            let mut file = File::open(framebuffer)
                .with_context(|| format!("open viewport framebuffer {}", framebuffer.display()))?;
            let mut bytes = [0_u8; 1024 * 1024];
            loop {
                let read = file.read(&mut bytes)?;
                if read == 0 {
                    break;
                }
                hasher.update(&bytes[..read]);
            }
            let framebuffer_hash = format!("{:x}", hasher.finalize());
            let output_name = self.viewport_output_name().to_owned();
            let output_area = output_geometry(output);
            let layers = self
                .layer_surfaces
                .iter()
                .filter_map(|layer| {
                    let role = match layer.namespace.as_str() {
                        "slopos-i-desktop" => "background",
                        "slopos-i-menu" => "menu",
                        "slopos-i-dock" => "dock",
                        _ => return None,
                    };
                    let local_x = layer.geo.loc.x.saturating_sub(output_area.x);
                    let local_y = layer.geo.loc.y.saturating_sub(output_area.y);
                    let configure_serial = layer.configure_serial;
                    let acknowledged = layer
                        .ack_serial
                        .is_some_and(|serial| serial == configure_serial && serial != 0);
                    Some(serde_json::json!({
                        "namespace": layer.namespace.clone(),
                        "layer": format!("{:?}", layer.layer).to_ascii_lowercase(),
                        "role": role,
                        "output": output_name.clone(),
                        "geometry_space": "logical",
                        "requested": {
                            "width": layer.requested.w.max(0),
                            "height": layer.requested.h.max(0)
                        },
                        "configured": {
                            "width": layer.geo.size.w.max(1),
                            "height": layer.geo.size.h.max(1)
                        },
                        "geometry": {
                            "x": local_x.max(0),
                            "y": local_y.max(0),
                            "width": layer.geo.size.w.max(1),
                            "height": layer.geo.size.h.max(1)
                        },
                        "active": layer.has_committed,
                        "configure_serial": configure_serial,
                        "acknowledged": acknowledged,
                        "ack_serial": layer.ack_serial.unwrap_or(0),
                        "committed": layer.has_committed,
                        "committed_frame_revision": layer.committed_frame_revision
                    }))
                })
                .collect::<Vec<_>>();
            let framebuffer_path = framebuffer.canonicalize().with_context(|| {
                format!(
                    "resolve compositor-owned viewport framebuffer {}",
                    framebuffer.display()
                )
            })?;
            let state = serde_json::json!({
                "schema_version": 1,
                "commit": env!("SLOPOS_BUILD_COMMIT"),
                "branch": env!("SLOPOS_BUILD_BRANCH"),
                "backend": self.display_backend_name(),
                "provenance": {"kind": "runtime", "capture": "compositor_framebuffer"},
                "coordinate_space": "logical",
                "output": {
                    "name": output_name,
                    "logical": {"width": logical_width, "height": logical_height},
                    "physical": {"width": physical_width, "height": physical_height},
                    "requested_scale": {"numerator": requested_scale.numerator, "denominator": requested_scale.denominator},
                    "effective_scale": {"numerator": effective_scale.numerator, "denominator": effective_scale.denominator},
                    "revision": self.outputs_revision.max(1),
                    "frame_revision": self.viewport_frame_revision
                },
                "framebuffer": {
                    "path": framebuffer_path,
                    "format": "png",
                    "dimensions": {"width": physical_width, "height": physical_height},
                    "sha256": framebuffer_hash,
                    "clear_color": [RETRO_GRAY.0, RETRO_GRAY.1, RETRO_GRAY.2, 255],
                    "clear_tolerance": 16
                },
                "layers": layers
            });
            let destination = runtime.join(VIEWPORT_STATE_FILE);
            fs::create_dir_all(&runtime)?;
            let temporary = destination.with_file_name(format!(
                ".{}.tmp-{}",
                VIEWPORT_STATE_FILE,
                std::process::id()
            ));
            let result = (|| -> anyhow::Result<()> {
                let file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    file.set_permissions(fs::Permissions::from_mode(0o600))?;
                }
                let mut writer = std::io::BufWriter::new(file);
                serde_json::to_writer_pretty(&mut writer, &state)?;
                writer.write_all(b"\n")?;
                writer.flush()?;
                writer.get_ref().sync_all()?;
                drop(writer);
                fs::rename(&temporary, &destination)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result
        }

        /// Repair per-display Space assignments whenever the authoritative
        /// connector inventory changes. A persisted connector name is only
        /// usable while that connector is present; clearing a stale value is
        /// safer than leaving a Space stranded on a removed output.
        fn reconcile_space_output_assignments(&mut self) {
            let output_names = self.output_names.clone();
            match self.spaces.reconcile_output_assignments(output_names) {
                Ok(cleared) if !cleared.is_empty() => {
                    tracing::info!(spaces = ?cleared, "cleared disconnected Space output assignments");
                    self.sync_legacy_workspace_state();
                    self.publish_spaces_state(true);
                    self.request_full_redraw();
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(%error, "could not reconcile Space output assignments")
                }
            }
        }

        fn display_backend_name(&self) -> &'static str {
            match self.backend_kind {
                CompositorBackendKind::NestedX11 => "nested",
                CompositorBackendKind::Headless => "headless",
                CompositorBackendKind::SessionDrm => "drm",
            }
        }

        fn supported_display_refresh_rates(&self) -> Vec<String> {
            let mut rates = vec![
                RefreshRate::Hz60.as_str().to_string(),
                RefreshRate::Hz120.as_str().to_string(),
                RefreshRate::Hz144.as_str().to_string(),
                RefreshRate::Hz165.as_str().to_string(),
            ];
            if self.vrr_supported {
                rates.push(RefreshRate::Adaptive.as_str().to_string());
            }
            rates
        }

        fn display_policy_snapshot(&self) -> DisplayPolicySnapshot {
            let effective_refresh = self.display_policy.effective_refresh_rate();
            DisplayPolicySnapshot {
                backend: self.display_backend_name().to_string(),
                revision: self.display_policy_revision,
                hdr_requested: self.display_policy.hdr_requested,
                hdr_supported: self.hdr_caps.hdr_supported,
                hdr_active: self.display_policy.hdr_requested
                    && self.hdr_caps.hdr_supported
                    && self.hdr_caps.current_color_space.is_hdr_encoding(),
                vrr_adaptive: matches!(effective_refresh, RefreshRate::Adaptive),
                vrr_supported: self.vrr_supported,
                refresh_rate_requested: self.display_policy.refresh_rate.as_str().to_string(),
                refresh_rate_applied: effective_refresh.as_str().to_string(),
                color_space_requested: self.display_policy.color_space.as_str().to_string(),
                color_space_applied: self.hdr_caps.current_color_space.as_str().to_string(),
                exact_match: self.display_policy_fallback_reason.is_none(),
                fallback_reason: self.display_policy_fallback_reason.clone(),
                runtime_mutation_supported: true,
                supported_refresh_rates: self.supported_display_refresh_rates(),
                supported_color_spaces: self
                    .hdr_caps
                    .supported_color_spaces
                    .iter()
                    .map(|space| space.as_str().to_string())
                    .collect(),
            }
        }

        fn publish_display_policy_state(&self) {
            if let Err(error) = write_display_policy_snapshot(&self.display_policy_snapshot()) {
                tracing::debug!(%error, "could not publish display-policy snapshot");
            }
        }

        fn apply_display_policy_request(&mut self, request: DisplayPolicyRequest) {
            let Some(refresh_rate) = RefreshRate::parse_flexible(&request.refresh_rate) else {
                tracing::warn!(value = %request.refresh_rate, "rejecting invalid display refresh policy");
                return;
            };
            let Some(color_space) = ColorSpace::from_str_flexible(&request.color_space) else {
                tracing::warn!(value = %request.color_space, "rejecting invalid display colour policy");
                return;
            };

            if request.hdr_requested && !self.hdr_caps.hdr_supported {
                tracing::warn!("rejecting HDR policy on a backend without verified HDR support");
                return;
            }
            if request.vrr_adaptive && !self.vrr_supported {
                tracing::warn!("rejecting VRR policy on a backend without verified VRR support");
                return;
            }
            if matches!(refresh_rate, RefreshRate::Adaptive) && !self.vrr_supported {
                tracing::warn!(
                    "rejecting adaptive refresh policy on a backend without verified VRR support"
                );
                return;
            }
            if !self.hdr_caps.supported_color_spaces.contains(&color_space) {
                tracing::warn!(value = %request.color_space, "rejecting unsupported display colour policy");
                return;
            }

            let mut next_caps = self.hdr_caps.clone();
            let outcome = next_caps.negotiate_request(request.hdr_requested, color_space);
            let effective_refresh = if request.vrr_adaptive {
                RefreshRate::Adaptive
            } else {
                refresh_rate
            };
            let previous_refresh_mhz = self.refresh_mhz;
            let next_refresh_mhz = match effective_refresh {
                RefreshRate::Adaptive => 60_000,
                rate => (rate.as_hz() as i32) * 1000,
            };

            self.hdr_caps = next_caps;
            self.display_policy = DisplayPolicy {
                hdr_requested: request.hdr_requested,
                vrr_adaptive: request.vrr_adaptive,
                refresh_rate,
                color_space,
            };
            self.display_policy_fallback_reason = match outcome.fallback_reason {
                HdrFallbackReason::None => None,
                HdrFallbackReason::HdrUnsupported => Some("hdr_unsupported".to_string()),
                HdrFallbackReason::RequestedColorSpaceUnsupported => {
                    Some("requested_color_space_unsupported".to_string())
                }
                HdrFallbackReason::SdrPolicyForcesSrgb => {
                    Some("sdr_policy_forces_srgb".to_string())
                }
                HdrFallbackReason::NoUsableHdrColorSpace => {
                    Some("no_usable_hdr_color_space".to_string())
                }
            };
            self.frame_scheduler.set_refresh_rate(effective_refresh);
            self.refresh_mhz = next_refresh_mhz;
            if previous_refresh_mhz != next_refresh_mhz {
                for (output, laid_out) in self.outputs.iter().zip(&self.laid_out_outputs) {
                    configure_output(output, laid_out, next_refresh_mhz, self.output_scale);
                }
            }
            self.display_policy_revision = self.display_policy_revision.saturating_add(1);
            self.publish_display_policy_state();
            self.request_full_redraw();
            tracing::info!(
                policy = %self.display_policy_snapshot().refresh_rate_applied,
                color_space = %self.display_policy_snapshot().color_space_applied,
                revision = self.display_policy_revision,
                "runtime display policy applied"
            );
        }

        fn reapply_application_policy(&mut self, app_id: &str) {
            let window_ids: Vec<String> = self
                .windows
                .iter()
                .filter(|window| window.app_id == app_id)
                .map(|window| window.window_id.clone())
                .collect();
            for window_id in window_ids {
                if let Err(error) = self
                    .spaces
                    .assign_window_for_application(window_id.clone(), app_id)
                {
                    tracing::warn!(%error, %app_id, %window_id, "could not apply Spaces application policy");
                }
            }
        }

        fn apply_spaces_command(&mut self, command: SpacesControlCommand) {
            let policy_app_id = match &command {
                SpacesControlCommand::SetApplicationPolicy { app_id, .. } => Some(app_id.clone()),
                _ => None,
            };
            let result = match command {
                SpacesControlCommand::Select { id } => SpaceId::new(id)
                    .ok_or(SpacesError::InvalidSpaceId(id))
                    .and_then(|id| self.spaces.select_space(id)),
                SpacesControlCommand::Create { name } => self.spaces.create_space(name),
                SpacesControlCommand::Rename { id, name } => SpaceId::new(id)
                    .ok_or(SpacesError::InvalidSpaceId(id))
                    .and_then(|id| self.spaces.rename_space(id, name).map(|()| id)),
                SpacesControlCommand::Reorder { id, order } => SpaceId::new(id)
                    .ok_or(SpacesError::InvalidSpaceId(id))
                    .and_then(|id| self.spaces.reorder_space(id, order).map(|()| id)),
                SpacesControlCommand::Remove { id } => SpaceId::new(id)
                    .ok_or(SpacesError::InvalidSpaceId(id))
                    .and_then(|id| self.spaces.remove_space(id)),
                SpacesControlCommand::MoveWindow { window_id, target } => {
                    let known_window_ids = self.known_space_window_ids();
                    if !known_window_ids.iter().any(|known| known == &window_id) {
                        return tracing::warn!(%window_id, "rejecting Spaces move for unknown window");
                    }
                    let target = match target {
                        SpaceTargetWire::Current => SpaceTarget::Current,
                        SpaceTargetWire::Id { id } => match SpaceId::new(id) {
                            Some(id) => SpaceTarget::Id(id),
                            None => {
                                tracing::warn!(id, "rejecting Spaces move with invalid Space ID");
                                return;
                            }
                        },
                        SpaceTargetWire::All => SpaceTarget::All,
                    };
                    self.spaces
                        .move_window(window_id, target)
                        .map(|()| self.spaces.active_space())
                }
                SpacesControlCommand::MoveActiveWindow { target } => {
                    let active_window_id = self.activated_window_id.clone().or_else(|| {
                        self.xwayland_keyboard_focus
                            .as_ref()
                            .map(|window| x11_space_window_id(window.window_id()))
                    });
                    let known_window_ids = self.known_space_window_ids();
                    self.spaces
                        .move_active_window(active_window_id.as_deref(), known_window_ids, target)
                        .map(|()| self.spaces.active_space())
                }
                SpacesControlCommand::MoveActiveWindowToOutput { output_id } => {
                    self.move_active_window_to_output(&output_id)
                }
                SpacesControlCommand::SetWallpaper { id, wallpaper } => SpaceId::new(id)
                    .ok_or(SpacesError::InvalidSpaceId(id))
                    .and_then(|id| self.spaces.set_wallpaper(id, wallpaper).map(|()| id)),
                SpacesControlCommand::SetAppearance { id, appearance } => SpaceId::new(id)
                    .ok_or(SpacesError::InvalidSpaceId(id))
                    .and_then(|id| self.spaces.set_appearance(id, appearance).map(|()| id)),
                SpacesControlCommand::SetClassification { id, classification } => SpaceId::new(id)
                    .ok_or(SpacesError::InvalidSpaceId(id))
                    .and_then(|id| {
                        self.spaces
                            .set_classification(
                                id,
                                fullscreen_classification_from_wire(classification),
                            )
                            .map(|()| id)
                    }),
                SpacesControlCommand::SetMultiMonitorPolicy { policy } => {
                    self.spaces
                        .set_multi_monitor_policy(multi_monitor_policy_from_wire(policy));
                    Ok(self.spaces.active_space())
                }
                SpacesControlCommand::AssignOutput { id, output_id } => {
                    let output_names = self.output_names.clone();
                    SpaceId::new(id)
                        .ok_or(SpacesError::InvalidSpaceId(id))
                        .and_then(|id| {
                            self.spaces
                                .set_space_output_with_inventory(id, output_id, output_names)
                                .map(|()| id)
                        })
                }
                SpacesControlCommand::SetApplicationPolicy { app_id, target } => {
                    application_target_from_wire(target).and_then(|target| {
                        self.spaces
                            .set_application_policy(app_id, target)
                            .map(|()| self.spaces.active_space())
                    })
                }
                SpacesControlCommand::RefreshThumbnails => {
                    self.thumbnail_refresh_requested = true;
                    Ok(self.spaces.active_space())
                }
            };

            match result {
                Ok(_) => {
                    if let Some(app_id) = policy_app_id {
                        self.reapply_application_policy(&app_id);
                    }
                    self.sync_legacy_workspace_state();
                    self.publish_spaces_state(true);
                    self.request_full_redraw();
                    self.apply_focus_after_workspace_switch();
                }
                Err(error) => tracing::warn!(%error, "rejecting Spaces command"),
            }
        }

        fn popup_origin(
            root_origin: Point<i32, Logical>,
            popup: &PopupKind,
            popup_offset: Point<i32, Logical>,
        ) -> Point<i32, Logical> {
            let geometry = popup.geometry();
            Point::from((
                root_origin.x + popup_offset.x - geometry.loc.x,
                root_origin.y + popup_offset.y - geometry.loc.y,
            ))
        }

        /// Find the topmost surface under a compositor-space point.
        ///
        /// Hit testing follows the committed surface trees rather than the
        /// compositor's configured rectangles. This preserves subsurface
        /// offsets, actual committed buffer sizes, and client input regions.
        fn layer_surface_under(
            layer: &MappedLayer,
            pt: Point<f64, Logical>,
        ) -> Option<(WlSurface, Point<f64, Logical>)> {
            for (popup, popup_offset) in
                PopupManager::popups_for_surface(layer.surface.wl_surface())
            {
                let origin = Self::popup_origin(layer.geo.loc, &popup, popup_offset);
                if let Some((surface, surface_origin)) =
                    under_from_surface_tree(popup.wl_surface(), pt, origin, WindowSurfaceType::ALL)
                {
                    return Some((surface, surface_origin.to_f64()));
                }
            }

            let local = Point::from((pt.x - layer.geo.loc.x as f64, pt.y - layer.geo.loc.y as f64));
            let (surface, origin) = under_from_surface_tree(
                layer.surface.wl_surface(),
                local,
                (0, 0),
                WindowSurfaceType::ALL,
            )?;
            Some((surface, layer_surface_hit_origin(layer.geo.loc, origin)))
        }

        pub(super) fn x11_surface_scene_origin(
            geometry: Rectangle<i32, Logical>,
        ) -> Point<i32, Logical> {
            geometry.loc
        }

        pub(super) fn x11_surface_scene_hit(
            geometry: Rectangle<i32, Logical>,
            point: Point<f64, Logical>,
        ) -> Option<Point<f64, Logical>> {
            let origin = Self::x11_surface_scene_origin(geometry);
            let width = f64::from(geometry.size.w.max(0));
            let height = f64::from(geometry.size.h.max(0));
            let local = Point::from((point.x - f64::from(origin.x), point.y - f64::from(origin.y)));
            (local.x >= 0.0 && local.y >= 0.0 && local.x < width && local.y < height)
                .then_some(local)
        }

        fn surface_under(
            &self,
            pt: Point<f64, Logical>,
        ) -> Option<(WlSurface, Point<f64, Logical>)> {
            for layer in self.layer_surfaces.iter().rev() {
                if matches!(layer.layer, Layer::Overlay | Layer::Top) {
                    if let Some(hit) = Self::layer_surface_under(layer, pt) {
                        return Some(hit);
                    }
                }
            }

            for (_window_id, surface, geometry) in self
                .x11_scene
                .associated_targets(&self.spaces)
                .into_iter()
                .rev()
            {
                if Self::x11_surface_scene_hit(geometry, pt).is_none() {
                    continue;
                }
                if let Some((surface, surface_origin)) = under_from_surface_tree(
                    &surface,
                    pt,
                    Self::x11_surface_scene_origin(geometry),
                    WindowSurfaceType::ALL,
                ) {
                    return Some((surface, surface_origin.to_f64()));
                }
            }

            for window in self
                .windows
                .iter()
                .rev()
                .filter(|w| !w.minimized && self.window_visible_on_active(&w.window_id))
            {
                for (popup, popup_offset) in
                    PopupManager::popups_for_surface(window.toplevel.wl_surface())
                {
                    let origin = Self::popup_origin(window.position, &popup, popup_offset);
                    if let Some((surface, surface_origin)) = under_from_surface_tree(
                        popup.wl_surface(),
                        pt,
                        origin,
                        WindowSurfaceType::ALL,
                    ) {
                        return Some((surface, surface_origin.to_f64()));
                    }
                }
                if let Some((surface, surface_origin)) = under_from_surface_tree(
                    window.toplevel.wl_surface(),
                    pt,
                    window.position,
                    WindowSurfaceType::ALL,
                ) {
                    return Some((surface, surface_origin.to_f64()));
                }
            }

            for layer in self.layer_surfaces.iter().rev() {
                if matches!(layer.layer, Layer::Bottom | Layer::Background) {
                    if let Some(hit) = Self::layer_surface_under(layer, pt) {
                        return Some(hit);
                    }
                }
            }
            self.headless_test_surface_under(pt)
        }

        /// Headless protocol clients deliberately commit a tiny SHM buffer,
        /// but the fallback also keeps the test deterministic if a client
        /// commits only an xdg role. It is never used by nested production or
        /// DRM input because the flag is set only with SLOPOS_TEST_INPUT=1 on
        /// the explicit headless backend.
        fn headless_test_surface_under(
            &self,
            pt: Point<f64, Logical>,
        ) -> Option<(WlSurface, Point<f64, Logical>)> {
            if !self.headless_test_input_enabled {
                return None;
            }
            self.windows
                .iter()
                .rev()
                .filter(|window| {
                    !window.minimized && self.window_visible_on_active(&window.window_id)
                })
                .find(|window| window.geometry().contains_f64(pt.x, pt.y))
                .map(|window| {
                    (
                        window.toplevel.wl_surface().clone(),
                        Point::from((window.position.x as f64, window.position.y as f64)),
                    )
                })
        }

        /// Resolve a surface to its mapped toplevel owner. Subsurfaces are
        /// normalized to their role-bearing tree root; popup roots are then
        /// accepted only when tracked under a known mapped toplevel.
        fn mapped_window_index_for_surface(&self, surface: &WlSurface) -> Option<usize> {
            let tree_root = surface_tree_root(surface);
            if let Some(index) = self
                .windows
                .iter()
                .position(|window| window.toplevel.wl_surface() == &tree_root)
            {
                return Some(index);
            }
            let popup = self.popup_manager.find_popup(&tree_root)?;
            let root = find_popup_root_surface(&popup).ok()?;
            self.windows
                .iter()
                .position(|window| window.toplevel.wl_surface() == &root)
        }

        fn popup_root_origin(&self, popup: &PopupKind) -> Option<Point<i32, Logical>> {
            let root = find_popup_root_surface(popup).ok()?;
            if let Some(window) = self
                .windows
                .iter()
                .find(|window| window.toplevel.wl_surface() == &root)
            {
                return Some(window.position);
            }
            self.layer_surfaces
                .iter()
                .find(|layer| layer.surface.wl_surface() == &root)
                .map(|layer| layer.geo.loc)
        }

        fn constrained_popup_geometry(
            &self,
            popup: &PopupKind,
            positioner: PositionerState,
        ) -> Rectangle<i32, Logical> {
            let Some(root_origin) = self.popup_root_origin(popup) else {
                return positioner.get_geometry();
            };
            let parent_offset = get_popup_toplevel_coords(popup);
            let output = self.output_area_for_point(root_origin);
            let target = Rectangle::new(
                Point::from((
                    output.x - root_origin.x - parent_offset.x,
                    output.y - root_origin.y - parent_offset.y,
                )),
                Size::from((output.width.max(1), output.height.max(1))),
            );
            positioner.get_unconstrained_geometry(target)
        }

        fn activated_window_for_surface(&self, surface: &WlSurface) -> Option<String> {
            self.mapped_window_index_for_surface(surface)
                .map(|index| self.windows[index].window_id.clone())
        }

        /// Keep xdg_toplevel.Activated synchronized with compositor focus.
        fn sync_activated_for_surface(&mut self, surface: Option<&WlSurface>) {
            let next = surface.and_then(|surface| self.activated_window_for_surface(surface));
            if self.activated_window_id == next {
                return;
            }
            let previous = self.activated_window_id.take();
            self.activated_window_id = next.clone();
            for window in &self.windows {
                let was_active = previous.as_ref() == Some(&window.window_id);
                let is_active = next.as_ref() == Some(&window.window_id);
                if !was_active && !is_active {
                    continue;
                }
                window.toplevel.with_pending_state(|state| {
                    if is_active {
                        state.states.set(xdg_toplevel::State::Activated);
                    } else {
                        state.states.unset(xdg_toplevel::State::Activated);
                    }
                });
                window.toplevel.send_configure();
            }
        }

        fn cleanup_popup_state(&mut self) {
            self.popup_manager.cleanup();
            if self.popup_grab.as_ref().is_some_and(PopupGrab::has_ended) {
                self.popup_grab = None;
            }
        }

        fn begin_popup_grab(
            &mut self,
            surface: PopupSurface,
            seat: wl_seat::WlSeat,
            serial: Serial,
        ) {
            let popup = PopupKind::from(surface);
            if !self.seat.owns(&seat) {
                tracing::debug!("rejecting xdg popup grab from an unknown seat");
                return;
            }
            let Some(root) = find_popup_root_surface(&popup).ok() else {
                tracing::debug!("rejecting xdg popup grab without a live root surface");
                return;
            };
            let popup_surface = popup.wl_surface().clone();
            let Ok(grab) = self
                .popup_manager
                .grab_popup(root, popup, &self.seat, serial)
            else {
                tracing::debug!("rejecting invalid xdg popup grab");
                return;
            };
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
            }
            if let Some(pointer) = self.seat.get_pointer() {
                pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
            }
            self.popup_grab = Some(grab);
            self.focus_surface(Some(popup_surface));
            self.request_redraw();
        }

        /// Bring window at `idx` to the top and focus keyboard+pointer on it.
        fn focus_window(&mut self, idx: usize) {
            if idx >= self.windows.len() {
                return;
            }
            let app_id = self.windows[idx].app_id.clone();
            self.windows[idx].minimized = false;
            // Rotate to top
            let surface = self.windows[idx].toplevel.wl_surface().clone();
            move_to_top(&mut self.windows, idx);

            self.focus_surface(Some(surface.clone()));
            if let Err(err) = slopos_compositor::publish_active_toplevel(Some(&app_id)) {
                tracing::debug!(error = %err, app_id = %app_id, "could not publish active application");
            }
            let serial = self.next_serial();
            // Move pointer focus to surface at (0,0) within the window
            if let Some(ptr) = self.seat.get_pointer() {
                let win = self.windows.last().unwrap();
                let local = Point::from((
                    (self.pointer_pos.x - win.position.x as f64),
                    (self.pointer_pos.y - win.position.y as f64),
                ));
                ptr.motion(
                    self,
                    Some((surface.clone(), local)),
                    &MotionEvent {
                        location: self.pointer_pos,
                        serial,
                        time: 0,
                    },
                );
                ptr.frame(self);
            }
        }

        fn focus_surface(&mut self, surface: Option<WlSurface>) {
            let keeps_interactive_grab = self.interactive_grab.as_ref().is_some_and(|grab| {
                surface.as_ref().is_some_and(|surface| {
                    self.windows.iter().any(|window| {
                        window.window_id == grab.window_id
                            && window.toplevel.wl_surface() == surface
                    })
                })
            });
            let keeps_x11_interactive_grab =
                self.x11_interactive_grab.as_ref().is_some_and(|grab| {
                    surface.as_ref().is_some_and(|surface| {
                        self.x11_scene
                            .associated_surface(grab.window_id)
                            .is_some_and(|associated| associated == *surface)
                    })
                });
            if (self.interactive_grab.is_some() || self.x11_interactive_grab.is_some())
                && !keeps_interactive_grab
                && !keeps_x11_interactive_grab
            {
                self.cancel_interactive_grab();
            }
            let x11_window = surface
                .as_ref()
                .and_then(|surface| self.x11_scene.window_for_surface(surface));
            let focused_x11_window = x11_window
                .as_ref()
                .filter(|window| {
                    self.x11_scene
                        .x11_surface_accepts_keyboard_focus(window.window_id())
                })
                .cloned();
            if let Some(window) = focused_x11_window.as_ref() {
                tracing::info!(
                    window = window.window_id(),
                    "XWayland keyboard focus selected"
                );
            } else if x11_window.is_some() {
                tracing::debug!("override-redirect surface kept out of keyboard focus");
            }
            self.x11_scene
                .set_active(focused_x11_window.as_ref().map(|window| window.window_id()));
            self.sync_activated_for_surface(surface.as_ref());
            if surface.is_none() {
                if let Err(err) = slopos_compositor::publish_active_toplevel(None) {
                    tracing::debug!(error = %err, "could not clear active application");
                }
            }
            let serial = self.next_serial();
            let previous_x11_focus = self.xwayland_keyboard_focus.take();
            let should_leave_x11 = previous_x11_focus.as_ref().is_some_and(|previous| {
                focused_x11_window
                    .as_ref()
                    .is_none_or(|next| next.window_id() != previous.window_id())
            });
            if should_leave_x11 {
                if let Some(previous) = previous_x11_focus.as_ref() {
                    let seat = self.seat.clone();
                    KeyboardTarget::leave(previous, &seat, self, serial);
                }
            }
            if let Some(next) = focused_x11_window.as_ref() {
                let seat = self.seat.clone();
                KeyboardTarget::enter(next, &seat, self, Vec::new(), serial);
                self.xwayland_keyboard_focus = Some(next.clone());
            } else if !should_leave_x11 {
                self.xwayland_keyboard_focus = previous_x11_focus;
            }
            if let Some(kb) = self.seat.get_keyboard() {
                kb.set_focus(self, surface.clone(), serial);
            }
            let client = surface.and_then(|surface| surface.client());
            set_data_device_focus(&self.display_handle, &self.seat, client.clone());
            set_primary_focus(&self.display_handle, &self.seat, client);
        }

        /// Retarget pointer focus immediately before button delivery.
        ///
        /// A button event carries no position. Replaying motion at the last
        /// compositor pointer location ensures Smithay sends the press to the
        /// surface currently under the click even when the backend did not
        /// report a motion immediately beforehand.
        fn forward_pointer_motion(&mut self, time: u32) {
            let pos = self.pointer_pos;
            let focus = self.surface_under(pos);
            let serial = self.next_serial();
            if let Some(ptr) = self.seat.get_pointer() {
                ptr.motion(
                    self,
                    focus,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time,
                    },
                );
                ptr.frame(self);
            }
        }

        /// Remove dead windows (client disconnected / surface destroyed).
        fn prune_dead_windows(&mut self) {
            let dead_ids: HashSet<String> = self
                .windows
                .iter()
                .filter(|window| !window.toplevel.alive())
                .map(|window| window.window_id.clone())
                .collect();
            if dead_ids.is_empty() {
                return;
            }

            if self
                .interactive_grab
                .as_ref()
                .is_some_and(|grab| dead_ids.contains(&grab.window_id))
            {
                self.cancel_interactive_grab();
            }
            if self
                .last_pointer_press
                .as_ref()
                .is_some_and(|press| dead_ids.contains(&press.window_id))
            {
                self.last_pointer_press = None;
                self.left_button_down = false;
            }

            let mut retained =
                Vec::with_capacity(self.windows.len().saturating_sub(dead_ids.len()));
            for window in self.windows.drain(..) {
                if dead_ids.contains(&window.window_id) {
                    self.spaces.remove_window(&window.window_id);
                    window.foreign.send_closed();
                } else {
                    retained.push(window);
                }
            }
            self.windows = retained;

            if self
                .last_minimized_window_id
                .as_ref()
                .is_some_and(|id| dead_ids.contains(id))
            {
                self.last_minimized_window_id = None;
            }

            self.request_full_redraw();
            self.sync_legacy_workspace_state();
            self.publish_spaces_state(true);
            self.apply_focus_after_workspace_switch();
        }

        /// After Super+workspace switch: unfocus windows now hidden; focus topmost
        /// visible window (paint order bottom→top). Clears keyboard focus when none.
        fn apply_focus_after_workspace_switch(&mut self) {
            let order: Vec<&str> = self
                .windows
                .iter()
                .filter(|w| !w.minimized)
                .map(|w| w.window_id.as_str())
                .collect();
            let target = order
                .iter()
                .rev()
                .find(|window_id| self.window_visible_on_active(window_id))
                .map(|window_id| (*window_id).to_owned());
            if let Some(id) = target {
                if let Some(idx) = self.windows.iter().position(|w| w.window_id == id) {
                    self.focus_window(idx);
                    return;
                }
            }
            if let Some((_, surface, _)) = self
                .x11_scene
                .associated_targets(&self.spaces)
                .into_iter()
                .next_back()
            {
                self.focus_surface(Some(surface));
                return;
            }
            // No visible window on this workspace — drop keyboard/selection focus so a
            // hidden window does not keep receiving keys.
            let serial = self.next_serial();
            if let Some(kb) = self.seat.get_keyboard() {
                kb.set_focus(self, None, serial);
            }
            self.sync_activated_for_surface(None);
            set_data_device_focus(&self.display_handle, &self.seat, None);
            set_primary_focus(&self.display_handle, &self.seat, None);
        }

        /// Return the visible Spaces overlay surface when it has explicitly
        /// requested keyboard focus.  The shell toggles the layer-shell
        /// keyboard-interactivity state together with the overlay geometry;
        /// keeping this decision in the compositor prevents the shell from
        /// receiving keys merely because it painted a local overview model.
        fn active_spaces_keyboard_surface(&self) -> Option<WlSurface> {
            self.layer_surfaces
                .iter()
                .rev()
                .find(|layer| {
                    layer.layer == Layer::Overlay
                        && layer.namespace == "slopos-i-spaces-overview"
                        && layer.geo.size.w > 1
                        && layer.geo.size.h > 1
                        && !matches!(
                            with_states(layer.surface.wl_surface(), |states| {
                                states
                                    .cached_state
                                    .get::<LayerSurfaceCachedState>()
                                    .current()
                                    .keyboard_interactivity
                            }),
                            KeyboardInteractivity::None
                        )
                })
                .map(|layer| layer.surface.wl_surface().clone())
        }

        /// Reconcile keyboard focus after a layer-shell commit.  Opening the
        /// live overview focuses the compositor-owned overlay; closing it
        /// restores the topmost visible ordinary client (or clears focus when
        /// no client remains).  This is intentionally keyed to the exact
        /// namespace and geometry rather than any generic OnDemand chrome.
        fn reconcile_spaces_keyboard_focus(&mut self) {
            let current_focus = self
                .seat
                .get_keyboard()
                .and_then(|keyboard| keyboard.current_focus());
            let current_is_spaces = current_focus.as_ref().is_some_and(|surface| {
                self.layer_surfaces.iter().any(|layer| {
                    layer.layer == Layer::Overlay
                        && layer.namespace == "slopos-i-spaces-overview"
                        && layer.surface.wl_surface() == surface
                })
            });

            if let Some(target) = self.active_spaces_keyboard_surface() {
                if current_focus.as_ref() != Some(&target) {
                    self.focus_surface(Some(target));
                }
            } else if current_is_spaces {
                self.apply_focus_after_workspace_switch();
            }
        }

        fn request_full_redraw(&mut self) {
            self.need_full_redraw = true;
            self.pending_damage = None;
            self.frame_dirty = true;
        }

        fn request_redraw(&mut self) {
            self.frame_dirty = true;
        }

        /// Convert a nested X11 host resize into the compositor's physical
        /// render-target extent.  Smithay drains this same resize event from
        /// `X11Surface::buffer`, so the state used by `renderer.render` must
        /// be updated before the next frame is painted.
        fn nested_x11_resize_output_size(new_size: Size<u16, Logical>) -> Size<i32, Physical> {
            Size::<i32, Physical>::from((
                i32::from(new_size.w).max(1),
                i32::from(new_size.h).max(1),
            ))
        }

        fn nested_x11_resize_logical_output_size(
            new_size: Size<u16, Logical>,
            scale: OutputScale,
        ) -> Size<i32, Logical> {
            let physical = Self::nested_x11_resize_output_size(new_size);
            let (width, height) = scale_physical_to_logical((physical.w, physical.h), scale);
            Size::<i32, Logical>::from((width.max(1), height.max(1)))
        }

        /// Convert a logical scene coordinate to the physical nested render
        /// target using the rational output scale.  Surface-tree locations are
        /// scene coordinates, not buffer coordinates, so applying this once at
        /// collection keeps integer and fractional scales on the same path.
        fn nested_logical_point_to_physical(
            point: Point<i32, Logical>,
            scale: OutputScale,
        ) -> Point<i32, Physical> {
            let scale_coordinate = |value: i32| {
                let scaled = i128::from(value) * i128::from(scale.numerator);
                let denominator = i128::from(scale.denominator);
                let rounded = if scaled >= 0 {
                    (scaled + denominator / 2) / denominator
                } else {
                    (scaled - denominator / 2) / denominator
                };
                rounded.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
            };
            Point::<i32, Physical>::from((scale_coordinate(point.x), scale_coordinate(point.y)))
        }

        /// Convert a logical placeholder extent to physical pixels without
        /// undersizing odd dimensions under a fractional scale.
        fn nested_logical_size_to_physical(
            size: Size<i32, Logical>,
            scale: OutputScale,
        ) -> Size<i32, Physical> {
            let (width, height) = scale_logical_to_physical((size.w, size.h), scale);
            Size::<i32, Physical>::from((width.max(0), height.max(0)))
        }

        /// Nested GLES accepts a rational scene scale.  Keep this separate
        /// from the integer `wl_output.scale` advertisement so viewport state
        /// records the actual framebuffer ratio rather than the protocol's
        /// compatibility quantisation.
        fn effective_nested_output_scale(requested: OutputScale) -> OutputScale {
            requested.reduced()
        }

        fn handle_nested_x11_resize(&mut self, new_size: Size<u16, Logical>) {
            let next = Self::nested_x11_resize_output_size(new_size);
            let previous = self.output_size;
            self.output_size = next;

            // The nested host canvas is the only physical output in the
            // single-output development topology. Keep the compositor's
            // logical output authority in lockstep with the new swapchain so
            // layer-shell surfaces, work areas, wl_output mode and client
            // hit-testing do not remain pinned to the startup dimensions.
            if self.laid_out_outputs.len() == 1 {
                let logical =
                    Self::nested_x11_resize_logical_output_size(new_size, self.output_scale);
                let mut output = self.laid_out_outputs[0];
                output.config.width = logical.w;
                output.config.height = logical.h;
                self.laid_out_outputs[0] = output;
                if let Some(wl_output) = self.outputs.first() {
                    configure_output(wl_output, &output, self.refresh_mhz, self.output_scale);
                }

                let output_area = output_geometry(&output);
                for layer in &mut self.layer_surfaces {
                    let (requested, anchor, margins, exclusive_zone) =
                        layer_surface_request(&layer.surface);
                    let local = layer_geometry_for(
                        &layer.namespace,
                        layer.layer,
                        Size::from((output_area.width, output_area.height)),
                        requested,
                        anchor,
                        margins,
                    );
                    layer.output_index = 0;
                    layer.geo = Rectangle::new(
                        Point::from((
                            output_area.x.saturating_add(local.loc.x),
                            output_area.y.saturating_add(local.loc.y),
                        )),
                        local.size,
                    );
                    layer.exclusive_zone = exclusive_zone;
                    layer
                        .surface
                        .with_pending_state(|state| state.size = Some(local.size));
                    Self::configure_layer(layer);
                }
                self.clamp_normal_windows_to_work_area();
                self.sync_all_window_output_membership();
                self.publish_outputs_state();
                if let Err(error) = slopos_compositor::publish_session_readiness(
                    &self.wayland_socket_name,
                    self.output_size.w,
                    self.output_size.h,
                ) {
                    tracing::debug!(%error, "could not publish nested resize readiness");
                }
            }

            self.pointer_pos.x = self
                .pointer_pos
                .x
                .clamp(0.0, f64::from(next.w.saturating_sub(1).max(0)));
            self.pointer_pos.y = self
                .pointer_pos
                .y
                .clamp(0.0, f64::from(next.h.saturating_sub(1).max(0)));

            // X11Surface::buffer() drops its old swapchain buffers when it
            // consumes ConfigureNotify. Always repaint the complete target,
            // including when the server repeats the same dimensions.
            self.request_full_redraw();
            tracing::info!(
                old_width = previous.w,
                old_height = previous.h,
                width = next.w,
                height = next.h,
                "nested host canvas resized"
            );
        }

        /// Collect real mapped client pixels for the currently selected Space
        /// without adding placeholder rectangles. This helper is used only by
        /// the thumbnail readback path; an unavailable or uncommitted client
        /// therefore contributes no fabricated window image.
        fn collect_space_thumbnail_elements(
            &self,
            renderer: &mut GlesRenderer,
            space: SpaceId,
        ) -> Vec<WaylandSurfaceRenderElement<GlesRenderer>> {
            let thumbnail_scale = (640.0 / f64::from(self.output_size.w.max(1)))
                .min(480.0 / f64::from(self.output_size.h.max(1)))
                .clamp(0.05, 1.0);
            let output_scale = Self::effective_nested_output_scale(self.output_scale).as_f64();
            let render_scale = output_scale * thumbnail_scale;
            let physical_point = |x: f64, y: f64| {
                Point::<i32, Physical>::from((
                    (x * render_scale).round() as i32,
                    (y * render_scale).round() as i32,
                ))
            };
            let mut elements = Vec::new();

            for layer in self
                .layer_surfaces
                .iter()
                .filter(|layer| matches!(layer.layer, Layer::Background | Layer::Bottom))
            {
                let loc = physical_point(f64::from(layer.geo.loc.x), f64::from(layer.geo.loc.y));
                elements.extend(render_elements_from_surface_tree(
                    renderer,
                    layer.surface.wl_surface(),
                    loc,
                    render_scale,
                    1.0_f32,
                    Kind::Unspecified,
                ));
            }

            for window in self.windows.iter().filter(|window| {
                !window.minimized && self.window_visible_on_space(&window.window_id, space)
            }) {
                let loc =
                    physical_point(f64::from(window.position.x), f64::from(window.position.y));
                elements.extend(render_elements_from_surface_tree(
                    renderer,
                    window.toplevel.wl_surface(),
                    loc,
                    render_scale,
                    1.0_f32,
                    Kind::Unspecified,
                ));
                for (popup, popup_offset) in
                    PopupManager::popups_for_surface(window.toplevel.wl_surface())
                {
                    let popup_loc = Self::popup_origin(window.position, &popup, popup_offset);
                    elements.extend(render_elements_from_surface_tree(
                        renderer,
                        popup.wl_surface(),
                        physical_point(f64::from(popup_loc.x), f64::from(popup_loc.y)),
                        render_scale,
                        1.0_f32,
                        Kind::Unspecified,
                    ));
                }
            }

            for (_, surface, geometry) in self
                .x11_scene
                .associated_targets_on_space(&self.spaces, space)
            {
                let origin = Self::x11_surface_scene_origin(geometry);
                elements.extend(render_elements_from_surface_tree(
                    renderer,
                    &surface,
                    physical_point(f64::from(origin.x), f64::from(origin.y)),
                    render_scale,
                    1.0_f32,
                    Kind::Unspecified,
                ));
            }
            elements
        }

        fn capture_space_thumbnails(&mut self) {
            if !self.thumbnail_refresh_requested {
                return;
            }
            self.thumbnail_refresh_requested = false;

            let Some(mut renderer) = self.renderer.take() else {
                tracing::debug!("Spaces thumbnails unavailable without a renderer");
                let manifest = SpaceThumbnailManifest {
                    session_epoch: self.spaces_session_epoch,
                    generation: self.spaces_revision,
                    captures: Vec::new(),
                };
                if let Err(error) = write_space_thumbnail_manifest(&manifest) {
                    tracing::warn!(%error, "could not clear unavailable Space thumbnails");
                }
                return;
            };
            let ids = self.spaces.space_ids();
            let thumbnail_scale = (640.0 / f64::from(self.output_size.w.max(1)))
                .min(480.0 / f64::from(self.output_size.h.max(1)))
                .clamp(0.05, 1.0);
            let output_scale = Self::effective_nested_output_scale(self.output_scale).as_f64();
            let render_scale = output_scale * thumbnail_scale;
            let capture_size = (
                (f64::from(self.output_size.w.max(1)) * thumbnail_scale).round() as i32,
                (f64::from(self.output_size.h.max(1)) * thumbnail_scale).round() as i32,
            );
            let mut captures = Vec::new();

            for id in ids {
                let elements = self.collect_space_thumbnail_elements(&mut renderer, id);
                let Some(path) = session_space_thumbnail_path(id.get()) else {
                    continue;
                };
                match slopos_compositor::screenshot::capture_to_path(
                    &mut renderer,
                    &elements,
                    capture_size,
                    render_scale,
                    [
                        RETRO_GRAY.0 as f32 / 255.0,
                        RETRO_GRAY.1 as f32 / 255.0,
                        RETRO_GRAY.2 as f32 / 255.0,
                        1.0,
                    ],
                    &path,
                ) {
                    Ok(_) => {
                        captures.push(SpaceThumbnailEntry {
                            space_id: id.get(),
                            width: capture_size.0 as u32,
                            height: capture_size.1 as u32,
                        });
                        tracing::info!(space = id.get(), path = %path.display(), "Space thumbnail captured");
                    }
                    Err(error) => {
                        tracing::warn!(space = id.get(), %error, "Space thumbnail capture failed")
                    }
                }
            }

            self.renderer = Some(renderer);
            let manifest = SpaceThumbnailManifest {
                session_epoch: self.spaces_session_epoch,
                generation: if captures.is_empty() {
                    self.spaces_revision
                } else {
                    self.spaces_revision.saturating_add(1)
                },
                captures,
            };
            if let Err(error) = write_space_thumbnail_manifest(&manifest) {
                tracing::warn!(%error, "could not publish Space thumbnail manifest");
            }
            if !manifest.captures.is_empty() {
                self.publish_spaces_state(false);
                self.request_full_redraw();
            }
        }

        /// Record dirty rects when a window moves/resizes (`accumulate_damage` over old+new).
        fn note_window_geometry_change(
            &mut self,
            window_id: &str,
            old: WindowGeometry,
            new: WindowGeometry,
        ) {
            if old == new {
                return;
            }
            if let Some(d) = accumulate_damage_for_window_move(window_id, old, new) {
                self.pending_damage = Some(accumulate_damage_rect(self.pending_damage, d));
            }
            let surface = self
                .windows
                .iter()
                .find(|window| window.window_id == window_id)
                .map(|window| window.toplevel.wl_surface().clone());
            if let Some(surface) = surface {
                self.sync_surface_output_membership(&surface, new);
            }
            self.frame_dirty = true;
        }

        /// Move a mapped window and accumulate damage over old+new extents.
        #[allow(dead_code)] // used when interactive move/shell rules land
        fn set_window_position(&mut self, idx: usize, x: i32, y: i32) {
            if idx >= self.windows.len() {
                return;
            }
            let old = self.windows[idx].geometry();
            self.windows[idx].position = Point::from((x, y));
            let new = self.windows[idx].geometry();
            let id = self.windows[idx].window_id.clone();
            self.note_window_geometry_change(&id, old, new);
        }

        /// Move the focused window to a compositor-owned output.
        ///
        /// The target connector and active surface are validated before any
        /// geometry or restore metadata is changed.  Native XDG toplevels and
        /// rootless XWayland windows both use the compositor-owned output
        /// migration policy; XWayland geometry is committed through the XWM
        /// surface before the scene registry is updated.
        fn move_active_window_to_output(
            &mut self,
            output_id: &str,
        ) -> Result<SpaceId, SpacesError> {
            let target_index = self
                .output_names
                .iter()
                .position(|name| name == output_id)
                .ok_or_else(|| SpacesError::InvalidOutputId(output_id.to_owned()))?;
            let target_output = self
                .laid_out_outputs
                .get(target_index)
                .map(output_geometry)
                .ok_or_else(|| SpacesError::InvalidOutputId(output_id.to_owned()))?;
            let target_work_area = self.work_area_for_output_index(target_index);

            let Some(active_window_id) = self.activated_window_id.clone() else {
                if let Some(focused_x11) = self
                    .xwayland_keyboard_focus
                    .as_ref()
                    .and_then(|window| self.x11_scene.visible_surface(window.window_id()))
                {
                    let x11_window_id = focused_x11.window_id();
                    let current_geometry = self
                        .x11_scene
                        .geometry(x11_window_id)
                        .map(|geometry| {
                            WindowGeometry::new(
                                geometry.loc.x,
                                geometry.loc.y,
                                geometry.size.w,
                                geometry.size.h,
                            )
                        })
                        .ok_or_else(|| {
                            SpacesError::InvalidWindowId(x11_space_window_id(x11_window_id))
                        })?;
                    let old_index =
                        output_index_for_geometry(&self.laid_out_outputs, current_geometry)
                            .unwrap_or(target_index);
                    let old_output = self
                        .laid_out_outputs
                        .get(old_index)
                        .map(output_geometry)
                        .unwrap_or(target_output);
                    let migration = plan_window_output_migration(
                        WindowPresentationState::Normal,
                        current_geometry,
                        None,
                        old_output,
                        target_output,
                        target_work_area,
                    );
                    let geometry = Rectangle::new(
                        Point::from((migration.geometry.x, migration.geometry.y)),
                        Size::from((migration.geometry.width, migration.geometry.height)),
                    );
                    focused_x11.configure(Some(geometry)).map_err(|error| {
                        SpacesError::InvalidWindowId(format!(
                            "XWayland window {x11_window_id} configure failed: {error}"
                        ))
                    })?;
                    self.x11_scene.configure(focused_x11, geometry);
                    self.sync_x11_scene_output_membership(x11_window_id);
                    self.request_full_redraw();
                    tracing::info!(
                        window = x11_window_id,
                        %output_id,
                        "moved focused XWayland window to output"
                    );
                    return Ok(self.spaces.active_space());
                }
                return Err(SpacesError::InvalidWindowId(String::new()));
            };
            let window_index = self
                .windows
                .iter()
                .position(|window| window.window_id == active_window_id)
                .ok_or_else(|| SpacesError::InvalidWindowId(active_window_id.clone()))?;
            let current_geometry = self.windows[window_index].geometry();
            let current_state = self.windows[window_index].presentation_state;
            let restore_state = self.windows[window_index].restore_state.clone();
            let old_index = output_index_for_geometry(&self.laid_out_outputs, current_geometry)
                .unwrap_or(target_index);
            let old_output = self
                .laid_out_outputs
                .get(old_index)
                .map(output_geometry)
                .unwrap_or(target_output);
            let migration = plan_window_output_migration(
                current_state,
                current_geometry,
                restore_state.as_ref(),
                old_output,
                target_output,
                target_work_area,
            );

            let old_geometry = current_geometry;
            self.windows[window_index].position =
                Point::from((migration.geometry.x, migration.geometry.y));
            self.windows[window_index].size =
                Size::from((migration.geometry.width, migration.geometry.height));
            if let Some(restore) = self.windows[window_index].restore_state.as_mut() {
                if let Some(normal_geometry) = migration.restore_geometry {
                    restore.normal_geometry = normal_geometry;
                }
                restore.output_id = output_id.to_owned();
            }
            let toplevel = self.windows[window_index].toplevel.clone();
            toplevel.with_pending_state(|state| {
                state.size = Some(Size::from((
                    migration.geometry.width,
                    migration.geometry.height,
                )));
            });
            toplevel.send_configure();
            self.note_window_geometry_change(&active_window_id, old_geometry, migration.geometry);
            self.sync_all_window_output_membership();
            self.request_full_redraw();
            tracing::info!(
                %active_window_id,
                %output_id,
                ?current_state,
                "moved focused native window to output"
            );
            Ok(self.spaces.active_space())
        }

        fn begin_interactive_grab(
            &mut self,
            surface: &ToplevelSurface,
            kind: InteractiveGrabKind,
            seat: &wl_seat::WlSeat,
            serial: Serial,
        ) {
            let requested_surface = surface.wl_surface();
            let Some((window_id, start_geometry, window_position)) = self
                .windows
                .iter()
                .find(|w| w.toplevel.wl_surface() == requested_surface)
                .map(|window| (window.window_id.clone(), window.geometry(), window.position))
            else {
                tracing::debug!(?kind, "rejecting interactive request for an unknown window");
                return;
            };
            let pressed_serial = self
                .last_pointer_press
                .as_ref()
                .map(|press| u32::from(press.serial));
            let pressed_window_id = self
                .last_pointer_press
                .as_ref()
                .map(|press| press.window_id.as_str());
            let same_client = match (requested_surface.client(), seat.client()) {
                (Some(surface_client), Some(seat_client)) => surface_client == seat_client,
                _ => false,
            };
            let authorized = pointer_grab_request_is_valid_for_window(
                u32::from(serial),
                pressed_serial,
                &window_id,
                pressed_window_id,
                self.left_button_down,
                self.seat.owns(seat),
                same_client,
            );
            if !authorized {
                tracing::debug!(
                    request_serial = u32::from(serial),
                    ?kind,
                    pressed_window_id,
                    requested_window_id = %window_id,
                    same_client,
                    "rejecting unauthorized xdg move/resize request"
                );
                return;
            }
            let Some(pointer) = self.seat.get_pointer() else {
                tracing::debug!(?kind, "rejecting interactive request without a pointer");
                return;
            };
            let pointer_location = pointer.current_location();
            let pointer_x = pointer_location.x.round() as i32;
            let pointer_y = pointer_location.y.round() as i32;
            self.interactive_grab = Some(InteractiveGrab {
                window_id: window_id.clone(),
                kind,
                start_pointer_x: pointer_x,
                start_pointer_y: pointer_y,
                start_geometry,
            });
            pointer.set_grab(
                self,
                InteractivePointerGrab {
                    start_data: GrabStartData {
                        focus: Some((
                            requested_surface.clone(),
                            Point::from((window_position.x as f64, window_position.y as f64)),
                        )),
                        button: 0x110,
                        location: pointer_location,
                    },
                },
                serial,
                Focus::Keep,
            );
            if matches!(kind, InteractiveGrabKind::Resize(_)) {
                surface.with_pending_state(|state| {
                    state.size = Some(Size::from((start_geometry.width, start_geometry.height)));
                    state.states.set(xdg_toplevel::State::Resizing);
                });
                surface.send_configure();
            }
            tracing::debug!(
                window_id = %window_id,
                ?kind,
                pointer_x,
                pointer_y,
                "interactive grab started"
            );
        }

        fn update_interactive_grab(&mut self) -> bool {
            if self.interactive_grab.is_none() {
                return self.update_x11_interactive_grab();
            }
            let Some(grab) = self.interactive_grab.clone() else {
                return false;
            };
            let Some(idx) = self
                .windows
                .iter()
                .position(|w| w.window_id == grab.window_id)
            else {
                self.finish_interactive_grab();
                return false;
            };
            let min_size = with_states(self.windows[idx].toplevel.wl_surface(), |states| {
                let mut cached = states.cached_state.get::<SurfaceCachedState>();
                cached.current().min_size
            });
            let new = geometry_for_interactive_grab(
                &grab,
                self.pointer_pos.x.round() as i32,
                self.pointer_pos.y.round() as i32,
                160.max(min_size.w),
                96.max(min_size.h),
                self.output_size.w,
                self.output_size.h,
            );
            let old = self.windows[idx].geometry();
            if old == new {
                return true;
            }
            self.windows[idx].position = Point::from((new.x, new.y));
            self.windows[idx].size = Size::from((new.width, new.height));
            let surface = self.windows[idx].toplevel.clone();
            let id = self.windows[idx].window_id.clone();
            if matches!(grab.kind, InteractiveGrabKind::Resize(_)) {
                surface.with_pending_state(|state| {
                    state.size = Some(Size::from((new.width, new.height)));
                    state.states.set(xdg_toplevel::State::Resizing);
                });
                surface.send_configure();
            }
            self.note_window_geometry_change(&id, old, new);
            true
        }

        fn update_x11_interactive_grab(&mut self) -> bool {
            let Some(grab) = self.x11_interactive_grab.clone() else {
                return false;
            };
            if !grab.surface.alive() || grab.surface.is_override_redirect() {
                self.finish_interactive_grab();
                return false;
            }
            let min_size = grab
                .surface
                .min_size()
                .unwrap_or_else(|| Size::from((160, 96)));
            let new = geometry_for_interactive_grab(
                &grab.policy,
                self.pointer_pos.x.round() as i32,
                self.pointer_pos.y.round() as i32,
                min_size.w.max(160),
                min_size.h.max(96),
                self.output_size.w,
                self.output_size.h,
            );
            let old = self
                .x11_scene
                .geometry(grab.window_id)
                .map(|geometry| {
                    WindowGeometry::new(
                        geometry.loc.x,
                        geometry.loc.y,
                        geometry.size.w,
                        geometry.size.h,
                    )
                })
                .unwrap_or(grab.policy.start_geometry);
            if old == new {
                return true;
            }
            let geometry = Rectangle::new(
                Point::from((new.x, new.y)),
                Size::from((new.width, new.height)),
            );
            if let Err(error) = grab.surface.configure(Some(geometry)) {
                tracing::debug!(
                    window = grab.window_id,
                    %error,
                    "XWayland interactive configure failed"
                );
                self.finish_interactive_grab();
                return false;
            }
            self.x11_scene.configure(grab.surface.clone(), geometry);
            self.sync_x11_scene_output_membership(grab.window_id);
            tracing::info!(
                window = grab.window_id,
                x = new.x,
                y = new.y,
                width = new.width,
                height = new.height,
                "XWayland surface geometry changed during grab"
            );
            self.request_full_redraw();
            true
        }

        fn finish_interactive_grab(&mut self) {
            let x11_grab = self.x11_interactive_grab.take();
            let native_grab = clear_interactive_grab_state(
                &mut self.interactive_grab,
                &mut self.last_pointer_press,
                &mut self.left_button_down,
            );
            if native_grab.is_none() && x11_grab.is_none() {
                return;
            }
            if let Some(grab) = native_grab {
                if matches!(grab.kind, InteractiveGrabKind::Resize(_)) {
                    if let Some(window) = self
                        .windows
                        .iter()
                        .find(|w| w.window_id == grab.window_id && w.toplevel.alive())
                    {
                        let surface = window.toplevel.clone();
                        surface.with_pending_state(|state| {
                            state.states.unset(xdg_toplevel::State::Resizing);
                            state.size = Some(window.size);
                        });
                        surface.send_configure();
                    }
                }
                tracing::debug!(window_id = %grab.window_id, "interactive grab finished");
            } else {
                self.last_pointer_press = None;
                self.left_button_down = false;
                if let Some(grab) = x11_grab {
                    tracing::debug!(
                        window = grab.window_id,
                        "XWayland interactive grab finished"
                    );
                }
            }
            self.request_redraw();
        }

        fn cancel_interactive_grab(&mut self) {
            if self.interactive_grab.is_some() || self.x11_interactive_grab.is_some() {
                if let Some(pointer) = self.seat.get_pointer() {
                    let serial = self.next_serial();
                    pointer.unset_grab(self, serial, 0);
                } else {
                    self.finish_interactive_grab();
                }
            } else {
                self.left_button_down = false;
                self.last_pointer_press = None;
            }
        }

        fn associated_x11_surface(&self, window: &X11WmSurface) -> Option<WlSurface> {
            self.x11_scene.associated_surface(window.window_id())
        }

        /// Register a rootless X11 window in the same authoritative Spaces
        /// model used by native Wayland toplevels.  Membership is assigned
        /// exactly once so a later Space move is never overwritten by an
        /// association/map callback.
        fn ensure_x11_space_membership(&mut self, window_id: X11Window) {
            let membership_id = x11_space_window_id(window_id);
            if !self.spaces.window_spaces(&membership_id).is_empty() {
                return;
            }
            if let Err(error) = self.spaces.assign_window_to_current(membership_id.clone()) {
                tracing::warn!(%error, %membership_id, "could not assign XWayland window to active Space");
                return;
            }
            tracing::info!(window = window_id, %membership_id, "XWayland window assigned to authoritative Space");
            self.sync_legacy_workspace_state();
            self.publish_spaces_state(true);
        }

        fn remove_x11_space_membership(&mut self, window_id: X11Window) {
            let membership_id = x11_space_window_id(window_id);
            if self.spaces.remove_window(&membership_id) {
                self.sync_legacy_workspace_state();
                self.publish_spaces_state(true);
            }
        }

        fn known_space_window_ids(&self) -> Vec<String> {
            self.windows
                .iter()
                .map(|window| window.window_id.clone())
                .chain(self.x11_scene.window_ids())
                .collect()
        }

        fn sync_x11_scene_output_membership(&self, window_id: X11Window) {
            let Some(surface) = self.x11_scene.associated_surface(window_id) else {
                return;
            };
            let Some(geometry) = self.x11_scene.geometry(window_id) else {
                return;
            };
            self.sync_surface_output_membership(
                &surface,
                WindowGeometry::new(
                    geometry.loc.x,
                    geometry.loc.y,
                    geometry.size.w,
                    geometry.size.h,
                ),
            );
        }

        fn leave_x11_scene_output_membership(&self, window_id: X11Window) {
            let Some(surface) = self.x11_scene.associated_surface(window_id) else {
                return;
            };
            for output in &self.outputs {
                output.leave(&surface);
            }
        }

        fn note_x11_surface_mapped(&mut self, window_id: X11Window) {
            if self.x11_scene.take_mapped_marker(window_id) {
                tracing::info!(window = window_id, "XWayland surface mapped into scene");
            }
        }

        fn x11_surface_has_keyboard_focus(&self, window_id: X11Window) -> bool {
            let Some(surface) = self.x11_scene.associated_surface(window_id) else {
                return false;
            };
            self.seat
                .get_keyboard()
                .and_then(|keyboard| keyboard.current_focus())
                .is_some_and(|focused| focused == surface)
        }

        fn xwayland_client_is_alive(&self, client_id: &ClientId) -> bool {
            let mut alive = false;
            self.display_handle
                .backend_handle()
                .with_all_clients(|candidate| {
                    if candidate == *client_id {
                        alive = true;
                    }
                });
            alive
        }

        fn recover_xwayland_startup(&mut self) {
            self.xwayland_client_id = None;
            if self.xwayland_recovery_budget.take_restart() {
                tracing::warn!(
                    remaining = self.xwayland_recovery_budget.remaining(),
                    "XWayland startup failed; restarting"
                );
                try_start_xwayland(self);
            } else {
                tracing::error!("XWayland startup recovery budget exhausted");
            }
        }

        fn ensure_xwayland_startup_watchdog(&mut self) {
            if self.xwayland_startup_watchdog_started {
                return;
            }
            self.xwayland_startup_watchdog_started = true;
            let result = self.loop_handle.insert_source(
                Timer::from_duration(Duration::from_millis(250)),
                |_, _, data| {
                    let startup_failed =
                        data.xwayland_client_id.as_ref().is_some_and(|client_id| {
                            data.xwm.is_none() && !data.xwayland_client_is_alive(client_id)
                        });
                    if startup_failed {
                        tracing::warn!("XWayland startup client exited before Ready; recovering");
                        data.recover_xwayland_startup();
                    }
                    TimeoutAction::ToDuration(Duration::from_millis(250))
                },
            );
            if let Err(error) = result {
                self.xwayland_startup_watchdog_started = false;
                tracing::warn!(%error, "failed to install XWayland startup watchdog");
            }
        }

        fn x11_client_seat(&self, surface: &WlSurface) -> Option<wl_seat::WlSeat> {
            let client = surface.client()?;
            self.seat.client_seats(&client).into_iter().next()
        }

        fn begin_x11_interactive_grab(
            &mut self,
            window: &X11WmSurface,
            kind: InteractiveGrabKind,
            button: u32,
        ) {
            let primary_button = button == 0x110 || button == 1;
            if !primary_button {
                tracing::debug!(
                    button,
                    ?kind,
                    "rejecting X11 interactive grab on non-primary button"
                );
                return;
            }

            let Some(surface) = self.associated_x11_surface(window) else {
                tracing::debug!(
                    window = window.window_id(),
                    ?kind,
                    "rejecting X11 interactive grab without associated wl_surface"
                );
                return;
            };
            let Some(seat) = self.x11_client_seat(&surface) else {
                tracing::debug!(
                    window = window.window_id(),
                    ?kind,
                    "rejecting X11 interactive grab without client seat resource"
                );
                return;
            };
            let Some(pressed) = self.last_pointer_press.as_ref() else {
                tracing::debug!(
                    window = window.window_id(),
                    ?kind,
                    "rejecting X11 interactive grab without prior pointer press"
                );
                return;
            };
            let pressed_serial = pressed.serial;
            let authorized = self.left_button_down
                && self.seat.owns(&seat)
                && pressed.x11_window_id == Some(window.window_id());
            if !authorized {
                tracing::debug!(
                    window = window.window_id(),
                    ?kind,
                    request_serial = u32::from(pressed_serial),
                    "rejecting unauthorized XWayland interactive grab"
                );
                return;
            }

            let geometry = window.geometry();
            let start_geometry = WindowGeometry::new(
                geometry.loc.x,
                geometry.loc.y,
                geometry.size.w,
                geometry.size.h,
            );
            let Some(pointer) = self.seat.get_pointer() else {
                tracing::debug!(
                    window = window.window_id(),
                    ?kind,
                    "rejecting X11 interactive request without a pointer"
                );
                return;
            };
            let pointer_location = pointer.current_location();
            let pointer_x = pointer_location.x.round() as i32;
            let pointer_y = pointer_location.y.round() as i32;
            let policy = match kind {
                InteractiveGrabKind::Move => InteractiveGrab::moving(
                    format!("x11:{}", window.window_id()),
                    pointer_x,
                    pointer_y,
                    start_geometry,
                ),
                InteractiveGrabKind::Resize(edges) => {
                    let Some(grab) = InteractiveGrab::resizing(
                        format!("x11:{}", window.window_id()),
                        edges,
                        pointer_x,
                        pointer_y,
                        start_geometry,
                    ) else {
                        return;
                    };
                    grab
                }
            };
            self.x11_interactive_grab = Some(X11InteractiveGrab {
                window_id: window.window_id(),
                surface: window.clone(),
                policy,
            });
            pointer.set_grab(
                self,
                InteractivePointerGrab {
                    start_data: GrabStartData {
                        focus: Some((
                            surface.clone(),
                            Point::from((geometry.loc.x as f64, geometry.loc.y as f64)),
                        )),
                        button: 0x110,
                        location: pointer_location,
                    },
                },
                pressed_serial,
                Focus::Keep,
            );
            tracing::info!(
                window = window.window_id(),
                ?kind,
                pointer_x,
                pointer_y,
                "XWayland interactive grab started"
            );
        }

        fn canvas_area(&self) -> WindowGeometry {
            WindowGeometry::new(0, 0, self.output_size.w, self.output_size.h)
        }

        fn output_area_for_index(&self, output_index: usize) -> WindowGeometry {
            self.laid_out_outputs
                .get(output_index)
                .map(output_geometry)
                .unwrap_or_else(|| self.canvas_area())
        }

        fn output_area_for_point(&self, point: Point<i32, Logical>) -> WindowGeometry {
            output_index_for_point(&self.laid_out_outputs, point.x, point.y)
                .map(|index| self.output_area_for_index(index))
                .unwrap_or_else(|| self.canvas_area())
        }

        fn output_index_for_resource(&self, requested: Option<&wl_output::WlOutput>) -> usize {
            requested
                .and_then(Output::from_resource)
                .and_then(|requested| self.outputs.iter().position(|output| output == &requested))
                .unwrap_or(0)
        }

        fn sync_surface_output_membership(&self, surface: &WlSurface, geometry: WindowGeometry) {
            let intersecting = intersecting_output_indices(&self.laid_out_outputs, geometry);
            for (index, output) in self.outputs.iter().enumerate() {
                if intersecting.contains(&index) {
                    output.enter(surface);
                } else {
                    output.leave(surface);
                }
            }
        }

        fn sync_surface_to_output(&self, surface: &WlSurface, output_index: usize) {
            for (index, output) in self.outputs.iter().enumerate() {
                if index == output_index {
                    output.enter(surface);
                } else {
                    output.leave(surface);
                }
            }
        }

        fn sync_all_window_output_membership(&self) {
            for window in &self.windows {
                self.sync_surface_output_membership(
                    window.toplevel.wl_surface(),
                    window.geometry(),
                );
            }
            for (window_id, _, geometry) in self.x11_scene.associated_surfaces() {
                if let Some(surface) = self.x11_scene.associated_surface(window_id) {
                    self.sync_surface_output_membership(
                        &surface,
                        WindowGeometry::new(
                            geometry.loc.x,
                            geometry.loc.y,
                            geometry.size.w,
                            geometry.size.h,
                        ),
                    );
                }
            }
        }

        fn work_area_for_output_index(&self, output_index: usize) -> WindowGeometry {
            let output = self.output_area_for_index(output_index);
            let reservations = self
                .layer_surfaces
                .iter()
                .filter(|layer| layer.output_index == output_index)
                .map(|layer| {
                    let (_, anchor, margins, _) = layer_surface_request(&layer.surface);
                    ExclusiveZoneReservation {
                        exclusive_zone: layer.exclusive_zone,
                        anchor_top: anchor.contains(Anchor::TOP),
                        anchor_bottom: anchor.contains(Anchor::BOTTOM),
                        anchor_left: anchor.contains(Anchor::LEFT),
                        anchor_right: anchor.contains(Anchor::RIGHT),
                        margin_top: margins.top,
                        margin_bottom: margins.bottom,
                        margin_left: margins.left,
                        margin_right: margins.right,
                    }
                });
            compute_exclusive_work_area(output, reservations)
        }

        /// Keep normal windows inside the current compositor-owned work area
        /// after a layer-shell surface changes its exclusive reservation.
        fn clamp_normal_windows_to_work_area(&mut self) {
            let fallback_work_area = self.work_area_for_output_index(0);
            let output_work_areas: Vec<WindowGeometry> = (0..self.laid_out_outputs.len())
                .map(|index| self.work_area_for_output_index(index))
                .collect();
            let mut changed = false;
            for window in &mut self.windows {
                if window.minimized
                    || window.presentation_state != WindowPresentationState::Normal
                    || window.app_id.starts_with("com.slopos.shell")
                {
                    continue;
                }
                let current = window.geometry();
                let work_area = output_index_for_geometry(&self.laid_out_outputs, current)
                    .and_then(|index| output_work_areas.get(index).copied())
                    .unwrap_or(fallback_work_area);
                let next = clamp_window_to_work_area(current, work_area);
                if current == next {
                    continue;
                }
                window.position = Point::from((next.x, next.y));
                window.size = Size::from((next.width, next.height));
                let toplevel = window.toplevel.clone();
                toplevel.with_pending_state(|state| {
                    state.size = Some(Size::from((next.width, next.height)));
                });
                toplevel.send_configure();
                changed = true;
            }
            if changed {
                self.sync_all_window_output_membership();
                self.request_full_redraw();
            }
        }

        fn runtime_scale_percent(&self) -> u32 {
            (self.output_scale.as_f64() * 100.0)
                .round()
                .clamp(1.0, 10_000.0) as u32
        }

        fn reconfigure_outputs(&mut self, layout: &str) -> Result<(), String> {
            let (new_names, new_layout) =
                validated_runtime_output_layout(layout, self.runtime_scale_percent())?;
            let new_total = total_output_size(&new_layout);
            let new_physical = apply_scale_to_output_config(new_total, self.output_scale);
            if self.x11_surface.is_some()
                && (new_physical.width != self.output_size.w
                    || new_physical.height != self.output_size.h)
            {
                return Err(format!(
                    "nested runtime topology must preserve the host canvas (current {}x{}, requested {}x{}); resize the nested host window first",
                    self.output_size.w,
                    self.output_size.h,
                    new_physical.width,
                    new_physical.height
                ));
            }

            let removed_count = self
                .output_names
                .iter()
                .filter(|name| !new_names.contains(name))
                .count();
            if self
                .disabled_output_globals
                .len()
                .saturating_add(removed_count)
                > MAX_DISABLED_OUTPUT_GLOBALS
            {
                return Err("too many retired output globals in this session; restart the compositor before another connector-removal cycle".to_owned());
            }

            self.cancel_interactive_grab();
            let old_layout = self.laid_out_outputs.clone();
            let old_names = self.output_names.clone();
            let old_canvas = self.canvas_area();
            let layer_output_names = self
                .layer_surfaces
                .iter()
                .map(|layer| {
                    old_names
                        .get(layer.output_index)
                        .cloned()
                        .unwrap_or_else(|| old_names.first().cloned().unwrap_or_default())
                })
                .collect::<Vec<_>>();

            // Clear old membership before any global is disabled. Retained
            // outputs are re-entered after the atomic topology replacement.
            let x11_surfaces = self.x11_scene.associated_surfaces();
            for output in &self.outputs {
                for window in &self.windows {
                    output.leave(window.toplevel.wl_surface());
                }
                for layer in &self.layer_surfaces {
                    output.leave(layer.surface.wl_surface());
                }
                for (_, surface, _) in &x11_surfaces {
                    output.leave(surface);
                }
            }

            let mut old_outputs = std::mem::take(&mut self.outputs);
            let mut old_globals = std::mem::take(&mut self.output_globals);
            let mut old_output_names = std::mem::take(&mut self.output_names);
            let mut old_laid_out = std::mem::take(&mut self.laid_out_outputs);
            let mut outputs = Vec::with_capacity(new_layout.len());
            let mut globals = Vec::with_capacity(new_layout.len());

            for (index, (name, laid_out)) in new_names.iter().zip(&new_layout).enumerate() {
                if let Some(old_index) = old_output_names.iter().position(|old| old == name) {
                    let output = old_outputs.remove(old_index);
                    let global = old_globals.remove(old_index);
                    old_output_names.remove(old_index);
                    old_laid_out.remove(old_index);
                    configure_output(&output, laid_out, self.refresh_mhz, self.output_scale);
                    outputs.push(output);
                    globals.push(global);
                } else {
                    let (output, global) = create_output(
                        &self.display_handle,
                        laid_out,
                        name.clone(),
                        index,
                        self.refresh_mhz,
                        self.output_scale,
                    );
                    outputs.push(output);
                    globals.push(global);
                }
            }

            for (output, global) in old_outputs.into_iter().zip(old_globals) {
                for window in &self.windows {
                    output.leave(window.toplevel.wl_surface());
                }
                for layer in &self.layer_surfaces {
                    output.leave(layer.surface.wl_surface());
                }
                for (_, surface, _) in &x11_surfaces {
                    output.leave(surface);
                }
                self.display_handle
                    .disable_global::<SloposCompositor>(global.clone());
                self.disabled_output_globals.push(global);
            }

            self.outputs = outputs;
            self.output_globals = globals;
            self.output_names = new_names;
            self.laid_out_outputs = new_layout;
            self.output_size =
                Size::<i32, Physical>::from((new_physical.width, new_physical.height));
            self.publish_outputs_state();
            self.reconcile_space_output_assignments();

            // Preserve each layer's connector identity when possible. A removed
            // connector deterministically falls back to the first active output.
            for (layer, old_name) in self.layer_surfaces.iter_mut().zip(layer_output_names) {
                let output_index = self
                    .output_names
                    .iter()
                    .position(|name| name == &old_name)
                    .unwrap_or(0);
                let output_area = self
                    .laid_out_outputs
                    .get(output_index)
                    .map(output_geometry)
                    .unwrap_or_else(|| WindowGeometry::new(0, 0, 1, 1));
                let output_size =
                    Size::<i32, Logical>::from((output_area.width, output_area.height));
                let (requested, anchor, margins, exclusive_zone) =
                    layer_surface_request(&layer.surface);
                let local = layer_geometry_for(
                    &layer.namespace,
                    layer.layer,
                    output_size,
                    requested,
                    anchor,
                    margins,
                );
                layer.output_index = output_index;
                layer.geo = Rectangle::new(
                    Point::from((
                        output_area.x.saturating_add(local.loc.x),
                        output_area.y.saturating_add(local.loc.y),
                    )),
                    local.size,
                );
                layer.exclusive_zone = exclusive_zone;
                layer.requested = requested;
                layer
                    .surface
                    .with_pending_state(|state| state.size = Some(local.size));
                Self::configure_layer(layer);
            }

            let work_areas = (0..self.laid_out_outputs.len())
                .map(|index| self.work_area_for_output_index(index))
                .collect::<Vec<_>>();
            for window in &mut self.windows {
                let old_geometry = window.geometry();
                let old_index = window
                    .restore_state
                    .as_ref()
                    .and_then(|restore| {
                        old_names.iter().position(|name| name == &restore.output_id)
                    })
                    .or_else(|| output_index_for_geometry(&old_layout, old_geometry))
                    .unwrap_or(0);
                let old_name = old_names.get(old_index).cloned().unwrap_or_default();
                let new_index = self
                    .output_names
                    .iter()
                    .position(|name| name == &old_name)
                    .unwrap_or(0);
                let old_output = old_layout
                    .get(old_index)
                    .map(output_geometry)
                    .unwrap_or(old_canvas);
                let new_output = self
                    .laid_out_outputs
                    .get(new_index)
                    .map(output_geometry)
                    .unwrap_or_else(|| WindowGeometry::new(0, 0, 1, 1));
                let work_area = work_areas.get(new_index).copied().unwrap_or(new_output);
                let remapped_current =
                    remap_geometry_between_outputs(old_geometry, old_output, new_output);
                let remapped_normal = window
                    .restore_state
                    .as_ref()
                    .map(|restore| {
                        remap_geometry_between_outputs(
                            restore.normal_geometry,
                            old_output,
                            new_output,
                        )
                    })
                    .unwrap_or(remapped_current);
                if let Some(restore) = window.restore_state.as_mut() {
                    restore.normal_geometry = clamp_window_to_work_area(remapped_normal, work_area);
                    restore.output_id = self
                        .output_names
                        .get(new_index)
                        .cloned()
                        .unwrap_or_else(|| format!("output-{new_index}"));
                }
                let next = match window.presentation_state {
                    WindowPresentationState::Normal => {
                        clamp_window_to_work_area(remapped_current, work_area)
                    }
                    WindowPresentationState::Minimized => {
                        clamp_window_to_work_area(remapped_current, work_area)
                    }
                    WindowPresentationState::Fullscreen => new_output,
                    state => calculate_presentation_geometry(
                        work_area,
                        state,
                        (state == WindowPresentationState::SmartZoomed)
                            .then_some((old_geometry.width, old_geometry.height)),
                        remapped_normal,
                    ),
                };
                window.position = Point::from((next.x, next.y));
                window.size = Size::from((next.width, next.height));
                window.toplevel.with_pending_state(|state| {
                    state.size = Some(Size::from((next.width, next.height)));
                });
                window.toplevel.send_configure();
            }

            self.pointer_pos.x = self
                .pointer_pos
                .x
                .clamp(0.0, f64::from(self.output_size.w.saturating_sub(1).max(0)));
            self.pointer_pos.y = self
                .pointer_pos
                .y
                .clamp(0.0, f64::from(self.output_size.h.saturating_sub(1).max(0)));
            self.sync_all_window_output_membership();
            let layer_membership = self
                .layer_surfaces
                .iter()
                .map(|layer| (layer.surface.wl_surface().clone(), layer.output_index))
                .collect::<Vec<_>>();
            for (surface, output_index) in layer_membership {
                self.sync_surface_to_output(&surface, output_index);
            }
            slopos_compositor::publish_session_readiness(
                &self.wayland_socket_name,
                self.output_size.w,
                self.output_size.h,
            )
            .map_err(|error| format!("update session readiness after topology change: {error}"))?;
            self.request_full_redraw();
            tracing::info!(
                outputs = self.outputs.len(),
                width = self.output_size.w,
                height = self.output_size.h,
                "runtime output topology applied"
            );
            Ok(())
        }

        fn apply_session_control_request(&mut self, request: SessionControlRequest) {
            match request {
                SessionControlRequest::FocusedWindow { action } => {
                    self.apply_focused_window_action(action);
                }
                SessionControlRequest::ActivateApplication { bundle_id } => {
                    self.activate_application(&bundle_id);
                }
                SessionControlRequest::SwitchWorkspace { index } => {
                    self.activate_workspace_index(index);
                }
                SessionControlRequest::Spaces { command } => {
                    self.apply_spaces_command(command);
                }
                SessionControlRequest::ReconfigureOutputs { layout } => {
                    if let Err(error) = self.reconfigure_outputs(&layout) {
                        tracing::warn!(%error, "runtime output topology rejected");
                    }
                }
                SessionControlRequest::SetDisplayPolicy { policy } => {
                    self.apply_display_policy_request(policy);
                }
                SessionControlRequest::CaptureScreenshot { destination } => {
                    match slopos_compositor::screenshot::request_capture_to(&destination) {
                        Ok(()) => self.request_full_redraw(),
                        Err(error) => tracing::warn!(
                            destination = %destination.display(),
                            %error,
                            "compositor screenshot request rejected"
                        ),
                    }
                }
                SessionControlRequest::HeadlessTestInput { event } => {
                    if !self.headless_test_input_enabled {
                        tracing::warn!(
                            "rejecting headless test input outside an explicitly enabled headless backend"
                        );
                        return;
                    }
                    self.apply_headless_test_input(event);
                    self.flush_headless_test_clients();
                }
                SessionControlRequest::FocusedApplicationMenu {
                    bundle_id,
                    action_id,
                } => {
                    tracing::warn!(
                        %bundle_id,
                        %action_id,
                        "application menu request reached compositor without an app endpoint"
                    );
                }
            }
        }

        fn apply_headless_test_input(&mut self, event: HeadlessInputEvent) {
            match event {
                HeadlessInputEvent::Motion { x, y, time_msec } => {
                    self.inject_headless_pointer_motion(x, y, time_msec)
                }
                HeadlessInputEvent::Button {
                    button,
                    pressed,
                    time_msec,
                } => self.inject_headless_pointer_button(
                    button,
                    if pressed {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                    time_msec,
                ),
                HeadlessInputEvent::GestureSwipeBegin { fingers, time_msec } => {
                    self.inject_headless_swipe_begin(fingers, time_msec)
                }
                HeadlessInputEvent::GestureSwipeUpdate {
                    delta_x,
                    delta_y,
                    time_msec,
                } => self.inject_headless_swipe_update(delta_x, delta_y, time_msec),
                HeadlessInputEvent::GestureSwipeEnd {
                    cancelled,
                    time_msec,
                } => self.inject_headless_swipe_end(cancelled, time_msec),
            }
        }

        fn inject_headless_swipe_begin(&mut self, fingers: u32, time: u32) {
            let serial = self.next_serial();
            self.workspace_swipe.begin(fingers);
            if let Some(pointer) = self.seat.get_pointer() {
                pointer.gesture_swipe_begin(
                    self,
                    &GestureSwipeBeginEvent {
                        serial,
                        time,
                        fingers,
                    },
                );
            }
            tracing::debug!(fingers, "headless swipe gesture began");
        }

        fn inject_headless_swipe_update(&mut self, delta_x: i32, delta_y: i32, time: u32) {
            let delta = Point::from((f64::from(delta_x), f64::from(delta_y)));
            self.workspace_swipe.update(delta.x, delta.y);
            if let Some(pointer) = self.seat.get_pointer() {
                pointer.gesture_swipe_update(self, &GestureSwipeUpdateEvent { time, delta });
            }
            self.request_full_redraw();
        }

        fn inject_headless_swipe_end(&mut self, cancelled: bool, time: u32) {
            let serial = self.next_serial();
            let action = self.workspace_swipe.end(cancelled);
            if let Some(pointer) = self.seat.get_pointer() {
                pointer.gesture_swipe_end(
                    self,
                    &GestureSwipeEndEvent {
                        serial,
                        time,
                        cancelled,
                    },
                );
            }
            if !cancelled {
                match action {
                    Some(WorkspaceSwipeAction::Next) => {
                        tracing::info!("headless three-finger swipe committed: next Space");
                        self.cycle_workspace_next();
                    }
                    Some(WorkspaceSwipeAction::Previous) => {
                        tracing::info!("headless three-finger swipe committed: previous Space");
                        self.cycle_workspace_prev();
                    }
                    None => {}
                }
            }
            self.request_full_redraw();
        }

        fn flush_headless_test_clients(&self) {
            let mut backend = self.display_handle.backend_handle();
            if let Err(error) = backend.flush(None) {
                tracing::warn!(%error, "headless test input client flush failed");
            }
        }

        fn inject_headless_pointer_motion(&mut self, x: i32, y: i32, time: u32) {
            let desired: Point<f64, Logical> = Point::from((f64::from(x), f64::from(y)));
            let logical: Point<f64, Logical> = Point::from((
                desired
                    .x
                    .clamp(0.0, f64::from(self.output_size.w.saturating_sub(1).max(0))),
                desired
                    .y
                    .clamp(0.0, f64::from(self.output_size.h.saturating_sub(1).max(0))),
            ));
            let current = self.pointer_pos;
            let delta = Point::from((logical.x - current.x, logical.y - current.y));
            let serial = self.next_serial();
            if let Some(ptr) = self.seat.get_pointer() {
                let relative_focus = self.surface_under(current);
                ptr.relative_motion(
                    self,
                    relative_focus,
                    &RelativeMotionEvent {
                        delta,
                        delta_unaccel: delta,
                        utime: u64::from(time) * 1_000,
                    },
                );
                let pos = constrain_pointer_destination(self, &ptr, current, logical);
                self.pointer_pos = pos;
                self.request_redraw();
                let focus = self.surface_under(pos);
                ptr.motion(
                    self,
                    focus,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time,
                    },
                );
                ptr.frame(self);
                maybe_activate_pointer_constraint(self, &ptr);
            } else {
                self.pointer_pos = logical;
                self.request_redraw();
            }
        }

        fn inject_headless_pointer_button(
            &mut self,
            button: u32,
            btn_state: ButtonState,
            time: u32,
        ) {
            let serial = self.next_serial();
            let primary_button = button == 0x110 || button == 1;
            if primary_button {
                self.left_button_down = btn_state == ButtonState::Pressed;
            }
            if btn_state == ButtonState::Pressed {
                let pos = self.pointer_pos;
                let hit = self.surface_under(pos);
                let mapped_window_index = hit
                    .as_ref()
                    .and_then(|(surface, _)| self.mapped_window_index_for_surface(surface));
                let x11_window = hit
                    .as_ref()
                    .and_then(|(surface, _)| self.x11_scene.window_for_surface(surface));
                if primary_button {
                    self.last_pointer_press = if let Some(index) = mapped_window_index {
                        Some(PointerPress {
                            serial,
                            window_id: self.windows[index].window_id.clone(),
                            x11_window_id: None,
                        })
                    } else {
                        x11_window
                            .as_ref()
                            .filter(|window| !window.is_override_redirect())
                            .map(|window| PointerPress {
                                serial,
                                window_id: String::new(),
                                x11_window_id: Some(window.window_id()),
                            })
                    };
                }
                match hit {
                    Some((surface, _)) => match mapped_window_index {
                        Some(idx) => self.focus_window(idx),
                        None if x11_window
                            .as_ref()
                            .is_some_and(|window| window.is_override_redirect()) =>
                        {
                            tracing::debug!(
                                window = x11_window
                                    .as_ref()
                                    .map(X11WmSurface::window_id)
                                    .unwrap_or_default(),
                                "override-redirect surface kept out of keyboard focus"
                            );
                        }
                        None => self.focus_surface(Some(surface)),
                    },
                    None => self.focus_surface(None),
                }
                self.forward_pointer_motion(time);
            }
            if let Some(ptr) = self.seat.get_pointer() {
                ptr.button(
                    self,
                    &ButtonEvent {
                        serial,
                        time,
                        button,
                        state: btn_state,
                    },
                );
                ptr.frame(self);
            }
            if primary_button && btn_state == ButtonState::Released {
                self.finish_interactive_grab();
            }
            self.request_redraw();
        }

        /// Activate a matching mapped client on behalf of shell chrome.
        ///
        /// The shell sends only a semantic application id; this backend owns
        /// the actual restore, stacking, focus, and active-toplevel update.
        fn activate_application(&mut self, bundle_id: &str) {
            let Some(idx) = self
                .windows
                .iter()
                .rposition(|window| window.app_id == bundle_id)
            else {
                tracing::debug!(%bundle_id, "application activation found no mapped client");
                return;
            };

            let window_id = self.windows[idx].window_id.clone();
            if self.windows[idx].minimized {
                let surface = self.windows[idx].toplevel.clone();
                self.set_window_presentation_state(&surface, WindowPresentationState::Normal);
                self.windows[idx].minimized = false;
                if self.last_minimized_window_id.as_deref() == Some(window_id.as_str()) {
                    self.last_minimized_window_id = None;
                }
            }
            self.focus_window(idx);
            tracing::info!(%bundle_id, %window_id, "activated existing application client");
        }

        fn apply_focused_window_action(&mut self, action: WindowPresentationAction) {
            let window_id = if action == WindowPresentationAction::Restore {
                self.last_minimized_window_id
                    .as_ref()
                    .and_then(|id| {
                        self.windows
                            .iter()
                            .find(|window| window.window_id == *id && window.minimized)
                            .map(|window| window.window_id.clone())
                    })
                    .or_else(|| self.activated_window_id.clone())
            } else {
                self.activated_window_id.clone()
            };
            let Some(window_id) = window_id else {
                tracing::debug!(
                    ?action,
                    "ignored focused-window action with no focused toplevel"
                );
                return;
            };
            let Some(idx) = self
                .windows
                .iter()
                .position(|window| window.window_id == window_id)
            else {
                tracing::debug!(%window_id, "focused-window action targeted a stale toplevel");
                return;
            };

            let current = self.windows[idx].presentation_state;
            let target = match action {
                WindowPresentationAction::ToggleZoom => {
                    if matches!(current, WindowPresentationState::Normal) {
                        WindowPresentationState::SmartZoomed
                    } else {
                        WindowPresentationState::Normal
                    }
                }
                WindowPresentationAction::SmartZoom => WindowPresentationState::SmartZoomed,
                WindowPresentationAction::Fill => WindowPresentationState::Filled,
                WindowPresentationAction::ToggleFullscreen => {
                    if current == WindowPresentationState::Fullscreen {
                        WindowPresentationState::Normal
                    } else {
                        WindowPresentationState::Fullscreen
                    }
                }
                WindowPresentationAction::Fullscreen => WindowPresentationState::Fullscreen,
                WindowPresentationAction::Minimize => WindowPresentationState::Minimized,
                WindowPresentationAction::Restore => WindowPresentationState::Normal,
                WindowPresentationAction::Close => {
                    self.windows[idx].toplevel.send_close();
                    tracing::info!(%window_id, "sent close request to focused toplevel");
                    return;
                }
            };

            let surface = self.windows[idx].toplevel.clone();
            self.set_window_presentation_state(&surface, target);
            self.windows[idx].minimized = target == WindowPresentationState::Minimized;
            if target == WindowPresentationState::Minimized {
                self.last_minimized_window_id = Some(window_id.clone());
            } else if target == WindowPresentationState::Normal
                && self.last_minimized_window_id.as_deref() == Some(window_id.as_str())
            {
                self.last_minimized_window_id = None;
            }
            tracing::info!(
                %window_id,
                ?action,
                state = ?target,
                "applied compositor presentation request"
            );
            if self.windows[idx].minimized {
                self.apply_focus_after_workspace_switch();
            } else if action == WindowPresentationAction::Restore {
                self.focus_window(idx);
            } else {
                self.request_redraw();
            }
        }

        fn set_window_presentation_state(
            &mut self,
            surface: &ToplevelSurface,
            target_state: WindowPresentationState,
        ) {
            let Some(idx) = self
                .windows
                .iter()
                .position(|w| w.toplevel.wl_surface() == surface.wl_surface())
            else {
                return;
            };
            let old = self.windows[idx].geometry();
            let current_state = self.windows[idx].presentation_state;
            let current_restore_state = self.windows[idx].restore_state.clone();
            let output_index = output_index_for_geometry(&self.laid_out_outputs, old).unwrap_or(0);
            let output_area = self
                .laid_out_outputs
                .get(output_index)
                .map(output_geometry)
                .unwrap_or_else(|| self.canvas_area());
            let work_area = self.work_area_for_output_index(output_index);
            let output_id = self
                .output_names
                .get(output_index)
                .cloned()
                .unwrap_or_else(|| format!("output-{output_index}"));
            let transition = transition_presentation_state(
                current_state,
                old,
                current_restore_state.as_ref(),
                target_state,
                work_area,
                output_area,
                None,
                output_id,
                self.spaces.active_index(),
            );
            self.windows[idx].presentation_state = transition.state;
            self.windows[idx].restore_state = transition.restore_state;
            self.windows[idx].position =
                Point::from((transition.geometry.x, transition.geometry.y));
            self.windows[idx].size =
                Size::from((transition.geometry.width, transition.geometry.height));
            let new = self.windows[idx].geometry();
            let toplevel = self.windows[idx].toplevel.clone();
            toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Maximized);
                state.states.unset(xdg_toplevel::State::Fullscreen);
                match target_state {
                    WindowPresentationState::Filled => {
                        state.states.set(xdg_toplevel::State::Maximized);
                    }
                    WindowPresentationState::Fullscreen => {
                        state.states.set(xdg_toplevel::State::Fullscreen);
                    }
                    _ => {}
                }
                state.size = Some(Size::from((new.width, new.height)));
            });
            toplevel.send_configure();
            let id = self.windows[idx].window_id.clone();
            self.note_window_geometry_change(&id, old, new);
        }

        fn cycle_workspace_next(&mut self) {
            self.spaces.cycle_next();
            self.sync_legacy_workspace_state();
            self.publish_spaces_state(true);
            self.request_full_redraw();
            eprintln!(
                "[slopos-compositor] {}",
                self.spaces_snapshot().active_space
            );
            self.apply_focus_after_workspace_switch();
        }

        fn cycle_workspace_prev(&mut self) {
            self.spaces.cycle_previous();
            self.sync_legacy_workspace_state();
            self.publish_spaces_state(true);
            self.request_full_redraw();
            eprintln!(
                "[slopos-compositor] {}",
                self.spaces_snapshot().active_space
            );
            self.apply_focus_after_workspace_switch();
        }

        fn activate_workspace_index(&mut self, index: u8) {
            let Some(space) = self.spaces.spaces().get(usize::from(index)) else {
                tracing::warn!(index, "rejecting invalid workspace activation request");
                return;
            };
            if self.spaces.activate_space(space.id()).is_ok() {
                self.sync_legacy_workspace_state();
                self.publish_spaces_state(true);
                self.request_full_redraw();
                eprintln!(
                    "[slopos-compositor] {}",
                    self.spaces_snapshot().active_space
                );
                self.apply_focus_after_workspace_switch();
            }
        }

        /// Render a frame using the GlesRenderer:
        ///   1. Acquire an X11 dmabuf
        ///   2. Bind it to the GL renderer
        ///   3. Clear to retro gray; composite layer-shell (under) → windows → layer-shell (over)
        ///   4. Finish the frame and present
        ///
        /// Client presentation honesty:
        /// - Prefer real SHM/client surface trees (`render_elements_from_surface_tree`).
        /// - Solid `WIN_COLORS` placeholders are used **only** for visible windows whose
        ///   surface tree yields zero elements (no committed buffer yet). They never
        ///   replace real content when a buffer has been committed.
        /// - Inactive-workspace windows are not painted (workspace filter).
        /// - Workspace switch requests a full redraw; window moves accumulate damage.
        fn render_frame(&mut self) {
            self.prune_dead_windows();
            self.cleanup_popup_state();
            self.layer_surfaces.retain(|l| l.surface.alive());
            self.capture_space_thumbnails();

            // Present plan: workspace switch forces full redraw; otherwise use pending
            // damage heuristic (still full clear today — partial clip is follow-on).
            let full_redraw = self.need_full_redraw
                || self
                    .pending_damage
                    .is_some_and(|d| prefer_full_redraw(d, self.output_size.w, self.output_size.h));
            self.need_full_redraw = false;
            let _damage_for_present = if full_redraw {
                None
            } else {
                self.pending_damage.take()
            };
            if full_redraw {
                self.pending_damage = None;
            }

            let cursor_status = self.cursor_status.clone();
            let cursor_position = self.pointer_pos;
            let active_space = self.spaces.active_space();
            let render_scale = Self::effective_nested_output_scale(self.output_scale);
            let render_scale_factor = render_scale.as_f64();
            let logical_to_physical = |point: Point<i32, Logical>| {
                Self::nested_logical_point_to_physical(point, render_scale)
            };
            let logical_size_to_physical = |size: Size<i32, Logical>| {
                Self::nested_logical_size_to_physical(size, render_scale)
            };
            let visible_window_ids: HashSet<String> = self
                .windows
                .iter()
                .filter(|window| {
                    !window.minimized
                        && self
                            .spaces
                            .window_spaces(&window.window_id)
                            .into_iter()
                            .any(|space| space == active_space)
                })
                .map(|window| window.window_id.clone())
                .collect();
            let visible_window_surfaces: Vec<_> = self
                .windows
                .iter()
                .filter(|window| visible_window_ids.contains(&window.window_id))
                .map(|window| window.toplevel.wl_surface().clone())
                .collect();
            let visible_x11_surfaces = self.x11_scene.associated_targets(&self.spaces);
            let (renderer, x11_surface) = match (self.renderer.as_mut(), self.x11_surface.as_mut())
            {
                (Some(r), Some(s)) => (r, s),
                _ => {
                    let now = self.clock.now();
                    if let Some(output) = self.outputs.first().cloned() {
                        for surface in &visible_window_surfaces {
                            send_frames_surface_tree(
                                surface,
                                &output,
                                now,
                                Some(Duration::ZERO),
                                |_, _| None,
                            );
                        }
                        for (_, surface, _) in &visible_x11_surfaces {
                            send_frames_surface_tree(
                                surface,
                                &output,
                                now,
                                Some(Duration::ZERO),
                                |_, _| None,
                            );
                        }
                        for layer in &self.layer_surfaces {
                            send_frames_surface_tree(
                                layer.surface.wl_surface(),
                                &output,
                                now,
                                Some(Duration::ZERO),
                                |_, _| None,
                            );
                        }
                    }
                    self.frame_dirty = false;
                    return;
                }
            };

            // Paint order: bottom layers → xdg windows → top/overlay layers.
            use slopos_compositor::{plan_compose_order, ChromeLayer};
            let layer_z: Vec<u8> = self
                .layer_surfaces
                .iter()
                .map(|l| match l.layer {
                    Layer::Background => ChromeLayer::Background.z_priority(),
                    Layer::Bottom => ChromeLayer::Bottom.z_priority(),
                    Layer::Top => ChromeLayer::Top.z_priority(),
                    Layer::Overlay => ChromeLayer::Overlay.z_priority(),
                })
                .collect();
            let compose = plan_compose_order(&layer_z);
            let under: Vec<usize> = compose
                .layer_indices_bottom_first
                .iter()
                .copied()
                .filter(|&i| layer_z.get(i).copied().unwrap_or(0) <= 1)
                .collect();
            let over: Vec<usize> = compose
                .layer_indices_bottom_first
                .iter()
                .copied()
                .filter(|&i| layer_z.get(i).copied().unwrap_or(0) > 1)
                .collect();

            // Collect SHM render elements BEFORE binding the render target.
            // Per-window: real surface elements when available; placeholders only when empty.
            let mut surface_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
            // (rect, color) for windows with no committed buffer on the active workspace.
            let mut placeholders: Vec<(Rectangle<i32, Physical>, Color32F)> = Vec::new();

            for &i in &under {
                let layer = &self.layer_surfaces[i];
                let popup_elements = PopupManager::popups_for_surface(layer.surface.wl_surface())
                    .flat_map(|(popup, popup_offset)| {
                        let popup_loc = Self::popup_origin(layer.geo.loc, &popup, popup_offset);
                        render_elements_from_surface_tree(
                            renderer,
                            popup.wl_surface(),
                            logical_to_physical(popup_loc),
                            render_scale_factor,
                            1.0_f32,
                            Kind::Unspecified,
                        )
                    });
                surface_elements.extend(popup_elements);
                let loc = logical_to_physical(layer.geo.loc);
                surface_elements.extend(render_elements_from_surface_tree(
                    renderer,
                    layer.surface.wl_surface(),
                    loc,
                    render_scale_factor,
                    1.0_f32,
                    Kind::Unspecified,
                ));
            }
            // Workspace filter: hide surfaces not on the active virtual desktop.
            let visible_windows: Vec<&MappedWindow> = self
                .windows
                .iter()
                .filter(|w| visible_window_ids.contains(&w.window_id))
                .collect();
            for (i, w) in visible_windows.iter().enumerate() {
                let loc = logical_to_physical(Point::from((w.position.x, w.position.y)));
                let popup_elements = PopupManager::popups_for_surface(w.toplevel.wl_surface())
                    .flat_map(|(popup, popup_offset)| {
                        let popup_loc = Self::popup_origin(w.position, &popup, popup_offset);
                        render_elements_from_surface_tree(
                            renderer,
                            popup.wl_surface(),
                            logical_to_physical(popup_loc),
                            render_scale_factor,
                            1.0_f32,
                            Kind::Unspecified,
                        )
                    });
                surface_elements.extend(popup_elements);
                let els = render_elements_from_surface_tree(
                    renderer,
                    w.toplevel.wl_surface(),
                    loc,
                    render_scale_factor,
                    1.0_f32,
                    Kind::Unspecified,
                );
                match window_paint_source(!els.is_empty()) {
                    WindowPaintSource::SurfaceTree => {
                        surface_elements.extend(els);
                    }
                    WindowPaintSource::Placeholder => {
                        // No committed buffer: solid rect so the window still appears.
                        let color_idx = i % WIN_COLORS.len();
                        let (r, g, b) = WIN_COLORS[color_idx];
                        let rect = Rectangle::new(
                            logical_to_physical(Point::from((w.position.x, w.position.y))),
                            logical_size_to_physical(Size::from((w.size.w, w.size.h))),
                        );
                        placeholders.push((rect, Color32F::from([r, g, b, 1.0_f32])));
                    }
                }
            }

            // X11 rootless windows are composed after normal Wayland windows and
            // before top/overlay chrome. The registry keeps their discovery order
            // stable while association/map callbacks prevent duplicate entries.
            let mut rendered_x11_windows = Vec::new();
            for (window_id, surface, geometry) in &visible_x11_surfaces {
                let loc = logical_to_physical(Self::x11_surface_scene_origin(*geometry));
                let elements = render_elements_from_surface_tree(
                    renderer,
                    surface,
                    loc,
                    render_scale_factor,
                    1.0_f32,
                    Kind::Unspecified,
                );
                if !elements.is_empty() {
                    surface_elements.extend(elements);
                    rendered_x11_windows.push(*window_id);
                }
            }
            for window_id in rendered_x11_windows {
                if self.x11_scene.take_rendered_marker(window_id) {
                    tracing::info!(window = window_id, "XWayland surface rendered");
                }
            }
            for &i in &over {
                let layer = &self.layer_surfaces[i];
                let popup_elements = PopupManager::popups_for_surface(layer.surface.wl_surface())
                    .flat_map(|(popup, popup_offset)| {
                        let popup_loc = Self::popup_origin(layer.geo.loc, &popup, popup_offset);
                        render_elements_from_surface_tree(
                            renderer,
                            popup.wl_surface(),
                            logical_to_physical(popup_loc),
                            render_scale_factor,
                            1.0_f32,
                            Kind::Unspecified,
                        )
                    });
                surface_elements.extend(popup_elements);
                let loc = logical_to_physical(layer.geo.loc);
                surface_elements.extend(render_elements_from_surface_tree(
                    renderer,
                    layer.surface.wl_surface(),
                    loc,
                    render_scale_factor,
                    1.0_f32,
                    Kind::Unspecified,
                ));
            }

            // Client cursor surfaces are real Wayland surfaces and must be the
            // top-most render element.  If a client does not provide one, the
            // permanent software fallback below is used.
            let mut client_cursor_drawn = false;
            if let CursorImageStatus::Surface(surface) = &cursor_status {
                let hotspot = with_states(surface, |states| {
                    states
                        .data_map
                        .get::<CursorImageSurfaceData>()
                        .and_then(|attrs| attrs.lock().ok().map(|attrs| attrs.hotspot))
                        .unwrap_or_else(|| Point::from((0, 0)))
                });
                let cursor_loc = logical_to_physical(Point::from((
                    cursor_position.x.round() as i32,
                    cursor_position.y.round() as i32,
                )));
                let hotspot = logical_to_physical(hotspot);
                let cursor_loc = Point::<i32, Physical>::from((
                    cursor_loc.x - hotspot.x,
                    cursor_loc.y - hotspot.y,
                ));
                let cursor_elements = render_elements_from_surface_tree(
                    renderer,
                    surface,
                    cursor_loc,
                    render_scale_factor,
                    1.0_f32,
                    Kind::Cursor,
                );
                client_cursor_drawn = !cursor_elements.is_empty();
                surface_elements.extend(cursor_elements);
            }

            // Acquire the next buffer from the X11 swapchain
            let capture_path = slopos_compositor::screenshot::capture_if_requested(
                renderer,
                &surface_elements,
                (self.output_size.w, self.output_size.h),
                render_scale_factor,
                [
                    RETRO_GRAY.0 as f32 / 255.0,
                    RETRO_GRAY.1 as f32 / 255.0,
                    RETRO_GRAY.2 as f32 / 255.0,
                    1.0_f32,
                ],
            );
            let (mut dmabuf, _age) = match x11_surface.buffer() {
                Ok(pair) => pair,
                Err(e) => {
                    eprintln!("[render] failed to get X11 buffer: {e}");
                    return;
                }
            };

            let output_size = self.output_size;

            // Bind the dmabuf as GL render target
            let mut target = match renderer.bind(&mut dmabuf) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[render] failed to bind dmabuf: {e}");
                    return;
                }
            };

            // Open a render frame
            let mut frame = match renderer.render(&mut target, output_size, Transform::Normal) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("[render] failed to start frame: {e}");
                    return;
                }
            };

            // Clear to retro gray: rgb(152, 152, 148) → linear ≈ (0.596, 0.596, 0.580)
            let retro_gray = Color32F::from([
                RETRO_GRAY.0 as f32 / 255.0,
                RETRO_GRAY.1 as f32 / 255.0,
                RETRO_GRAY.2 as f32 / 255.0,
                1.0_f32,
            ]);
            let full_screen = Rectangle::new(Point::<i32, Physical>::from((0, 0)), output_size);
            if let Err(e) = frame.clear(retro_gray, &[full_screen]) {
                eprintln!("[render] clear failed: {e}");
            }

            if !placeholders.is_empty() && self.placeholder_stats.note_frame_with_placeholders() {
                eprintln!(
                    "[slopos-compositor] present honesty: frame used solid placeholders \
                     (no committed SHM buffer for {} window(s)); session counter starts at {}",
                    placeholders.len(),
                    self.placeholder_stats.frames_with_placeholders
                );
            }
            for (rect, color) in &placeholders {
                if let Err(e) = frame.clear(*color, &[*rect]) {
                    eprintln!("[render] window placeholder clear failed: {e}");
                }
            }

            if !surface_elements.is_empty() {
                surface_elements.reverse();
                if let Err(e) = draw_render_elements::<GlesRenderer, _, _>(
                    &mut frame,
                    render_scale_factor,
                    &surface_elements,
                    &[full_screen],
                ) {
                    eprintln!("[render] draw_render_elements failed: {e}");
                }
            }

            // Permanent compositor-owned software cursor fallback.  It remains
            // visible for Named cursors and whenever a client cursor surface has
            // not committed a buffer. Hidden is respected exactly.
            let fallback_cursor =
                !matches!(cursor_status, CursorImageStatus::Hidden) && !client_cursor_drawn;
            if fallback_cursor {
                let origin_x = cursor_position.x.round() as i32;
                let origin_y = cursor_position.y.round() as i32;
                let black = Color32F::from([0.0, 0.0, 0.0, 1.0]);
                let white = Color32F::from([1.0, 1.0, 1.0, 1.0]);
                // Classic high-contrast arrow, represented as horizontal runs.
                const OUTLINE: &[(i32, i32, i32)] = &[
                    (0, 0, 1),
                    (0, 1, 2),
                    (0, 2, 3),
                    (0, 3, 4),
                    (0, 4, 5),
                    (0, 5, 6),
                    (0, 6, 7),
                    (0, 7, 8),
                    (0, 8, 9),
                    (0, 9, 10),
                    (0, 10, 11),
                    (0, 11, 12),
                    (0, 12, 8),
                    (0, 13, 5),
                    (0, 14, 4),
                    (0, 15, 3),
                    (5, 12, 4),
                    (6, 13, 4),
                    (7, 14, 4),
                    (8, 15, 4),
                    (9, 16, 3),
                    (10, 17, 3),
                ];
                const FILL: &[(i32, i32, i32)] = &[
                    (1, 2, 1),
                    (1, 3, 2),
                    (1, 4, 3),
                    (1, 5, 4),
                    (1, 6, 5),
                    (1, 7, 6),
                    (1, 8, 7),
                    (1, 9, 8),
                    (1, 10, 9),
                    (1, 11, 6),
                    (1, 12, 3),
                ];
                for &(x, y, width) in OUTLINE {
                    let start = logical_to_physical(Point::from((origin_x + x, origin_y + y)));
                    let end =
                        logical_to_physical(Point::from((origin_x + x + width, origin_y + y + 1)));
                    let rect = Rectangle::new(
                        start,
                        Size::<i32, Physical>::from((
                            (end.x - start.x).max(1),
                            (end.y - start.y).max(1),
                        )),
                    );
                    let _ = frame.clear(black, &[rect]);
                }
                for &(x, y, width) in FILL {
                    let start = logical_to_physical(Point::from((origin_x + x, origin_y + y)));
                    let end =
                        logical_to_physical(Point::from((origin_x + x + width, origin_y + y + 1)));
                    let rect = Rectangle::new(
                        start,
                        Size::<i32, Physical>::from((
                            (end.x - start.x).max(1),
                            (end.y - start.y).max(1),
                        )),
                    );
                    let _ = frame.clear(white, &[rect]);
                }
            }

            // Finish the frame (flushes GL commands)
            let frame_finished = match frame.finish() {
                Ok(_) => true,
                Err(e) => {
                    eprintln!("[render] frame finish failed: {e}");
                    false
                }
            };

            // Present to the X11 window
            let submitted = match x11_surface.submit() {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("[render] submit failed: {e}");
                    false
                }
            };

            if frame_finished && submitted {
                self.viewport_frame_revision = self.viewport_frame_revision.saturating_add(1);
                for layer in &mut self.layer_surfaces {
                    if layer.has_committed {
                        layer.committed_frame_revision = self.viewport_frame_revision;
                    }
                }
                if let Some(path) = capture_path {
                    if let Err(error) = self.publish_viewport_state(&path) {
                        tracing::warn!(%error, "could not publish runtime viewport state");
                    }
                }
            }

            // Release frame callbacks for everything we just presented. Clients
            // that throttle drawing on wl_surface.frame (winit/wgpu apps, and
            // therefore every SLOPOS-I app) render exactly one frame and then
            // wait forever without this.
            let now = self.clock.now();
            for window in self.windows.iter().filter(|window| {
                !window.minimized && self.window_visible_on_active(&window.window_id)
            }) {
                let output_index =
                    output_index_for_geometry(&self.laid_out_outputs, window.geometry())
                        .unwrap_or(0);
                if let Some(output) = self.outputs.get(output_index) {
                    send_frames_surface_tree(
                        window.toplevel.wl_surface(),
                        output,
                        now,
                        Some(Duration::ZERO),
                        |_, _| None,
                    );
                }
            }
            for (_, surface, geometry) in self.x11_scene.associated_targets(&self.spaces) {
                let output_index = output_index_for_geometry(
                    &self.laid_out_outputs,
                    WindowGeometry::new(
                        geometry.loc.x,
                        geometry.loc.y,
                        geometry.size.w,
                        geometry.size.h,
                    ),
                )
                .unwrap_or(0);
                if let Some(output) = self.outputs.get(output_index) {
                    send_frames_surface_tree(
                        &surface,
                        output,
                        now,
                        Some(Duration::ZERO),
                        |_, _| None,
                    );
                }
            }
            for layer in &self.layer_surfaces {
                let Some(output) = self.outputs.get(layer.output_index) else {
                    continue;
                };
                send_frames_surface_tree(
                    layer.surface.wl_surface(),
                    output,
                    now,
                    Some(Duration::ZERO),
                    |_, _| None,
                );
                for (popup, _) in PopupManager::popups_for_surface(layer.surface.wl_surface()) {
                    send_frames_surface_tree(
                        popup.wl_surface(),
                        output,
                        now,
                        Some(Duration::ZERO),
                        |_, _| None,
                    );
                }
            }

            self.frame_scheduler.record_frame();
            self.frame_dirty = false;
        }
    }

    // -----------------------------------------------------------------------
    // BufferHandler (required by on_commit_buffer_handler)
    // -----------------------------------------------------------------------

    impl BufferHandler for SloposCompositor {
        fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
    }

    // -----------------------------------------------------------------------
    // CompositorHandler
    // -----------------------------------------------------------------------

    impl CompositorHandler for SloposCompositor {
        fn compositor_state(&mut self) -> &mut CompositorState {
            &mut self.compositor_state
        }

        fn client_compositor_state<'a>(
            &self,
            client: &'a smithay::reexports::wayland_server::Client,
        ) -> &'a CompositorClientState {
            if let Some(state) = client.get_data::<ClientState>() {
                &state.compositor_state
            } else if let Some(state) = client.get_data::<XWaylandClientData>() {
                // Smithay owns the XWayland bridge client and stores its
                // compositor state in XWaylandClientData rather than our
                // ordinary ClientState.
                &state.compositor_state
            } else {
                panic!("Wayland client is missing compositor client state")
            }
        }

        fn commit(&mut self, surface: &WlSurface) {
            on_commit_buffer_handler::<Self>(surface);
            self.popup_manager.commit(surface);
            // Update size of the matching window after the client commits.
            // ToplevelSurface::current_state gives us the server-side acknowledged size;
            // use that or fall back to DEFAULT_WIN. Size changes accumulate damage.
            let mut geometry_change: Option<(String, WindowGeometry, WindowGeometry)> = None;
            for w in self.windows.iter_mut() {
                if w.toplevel.wl_surface() == surface {
                    let old = w.geometry();
                    let st = w.toplevel.current_state();
                    let (sw, sh) = (
                        if st.size.map_or(0, |s| s.w) > 0 {
                            st.size.unwrap().w
                        } else {
                            DEFAULT_WINDOW_W
                        },
                        if st.size.map_or(0, |s| s.h) > 0 {
                            st.size.unwrap().h
                        } else {
                            DEFAULT_WINDOW_H
                        },
                    );
                    w.size = Size::from((sw, sh));
                    let new = w.geometry();
                    if old != new {
                        geometry_change = Some((w.window_id.clone(), old, new));
                    }
                    break;
                }
            }
            if let Some((id, old, new)) = geometry_change {
                self.note_window_geometry_change(&id, old, new);
            }

            // Apply the client-requested layer-shell anchors, margins, and
            // size relative to the exact output selected when the layer was created.
            let laid_out_outputs = self.laid_out_outputs.clone();
            let fallback_canvas = self.canvas_area();
            let mut layer_membership = None;
            for layer in self.layer_surfaces.iter_mut() {
                if layer.surface.wl_surface() != surface {
                    continue;
                }
                let output_area = laid_out_outputs
                    .get(layer.output_index)
                    .map(output_geometry)
                    .unwrap_or(fallback_canvas);
                let output_size =
                    Size::<i32, Logical>::from((output_area.width, output_area.height));
                let (requested, anchor, margins, exclusive_zone) =
                    layer_surface_request(&layer.surface);
                let local_geo = layer_geometry_for(
                    &layer.namespace,
                    layer.layer,
                    output_size,
                    requested,
                    anchor,
                    margins,
                );
                let geo = Rectangle::new(
                    Point::from((
                        output_area.x.saturating_add(local_geo.loc.x),
                        output_area.y.saturating_add(local_geo.loc.y),
                    )),
                    local_geo.size,
                );
                let current = layer.surface.current_state();
                if current.size == Some(geo.size) {
                    layer.has_committed = true;
                    if layer.configure_serial != 0 {
                        layer.ack_serial = Some(layer.configure_serial);
                    }
                }
                if current.size != Some(geo.size) {
                    layer.surface.with_pending_state(|state| {
                        state.size = Some(geo.size);
                    });
                    Self::configure_layer(layer);
                }
                layer.geo = geo;
                layer.exclusive_zone = exclusive_zone;
                layer.requested = requested;
                layer_membership = Some((layer.surface.wl_surface().clone(), layer.output_index));
                break;
            }
            if let Some((surface, output_index)) = layer_membership {
                self.sync_surface_to_output(&surface, output_index);
            }
            self.reconcile_spaces_keyboard_focus();
            self.clamp_normal_windows_to_work_area();
            self.request_redraw();
        }
    }

    delegate_compositor!(SloposCompositor);

    // -----------------------------------------------------------------------
    // ShmHandler
    // -----------------------------------------------------------------------

    impl ShmHandler for SloposCompositor {
        fn shm_state(&self) -> &ShmState {
            &self.shm_state
        }
    }

    delegate_shm!(SloposCompositor);

    // -----------------------------------------------------------------------
    // SeatHandler
    // -----------------------------------------------------------------------

    impl SeatHandler for SloposCompositor {
        type KeyboardFocus = WlSurface;
        type PointerFocus = WlSurface;
        type TouchFocus = WlSurface;

        fn seat_state(&mut self) -> &mut SeatState<SloposCompositor> {
            &mut self.seat_state
        }

        fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
            self.cursor_status = image;
            self.request_redraw();
        }

        fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
            let client = focused.and_then(|s| s.client());
            set_data_device_focus(&self.display_handle, seat, client.clone());
            set_primary_focus(&self.display_handle, seat, client);
        }
    }

    delegate_seat!(SloposCompositor);
    delegate_relative_pointer!(SloposCompositor);
    delegate_pointer_constraints!(SloposCompositor);

    impl PointerConstraintsHandler for SloposCompositor {
        fn new_constraint(&mut self, _surface: &WlSurface, pointer: &PointerHandle<Self>) {
            maybe_activate_pointer_constraint(self, pointer);
        }

        fn cursor_position_hint(
            &mut self,
            _surface: &WlSurface,
            _pointer: &PointerHandle<Self>,
            _location: Point<f64, Logical>,
        ) {
            // The unstable-v1 protocol defines this as an optional compositor
            // warp hint. The nested X11 backend has no raw host-pointer warp
            // primitive here, so retaining normal host cursor ownership is the
            // least surprising and standards-compliant behaviour.
        }
    }

    // -----------------------------------------------------------------------
    // SelectionHandler / DataDeviceHandler (P1.1)
    // -----------------------------------------------------------------------

    /// Write mime payload to the client-provided fd on a background thread so the
    /// compositor event loop never blocks on a full pipe. Missing data → EOF only.
    fn write_selection_fd(mime_type: String, fd: OwnedFd, data: Option<Vec<u8>>) {
        if let Err(err) = std::thread::Builder::new()
            .name("selection-send".into())
            .spawn(move || {
                let mut file = std::fs::File::from(fd);
                if let Some(bytes) = data {
                    if let Err(err) = file.write_all(&bytes) {
                        if matches!(
                            err.kind(),
                            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                        ) {
                            tracing::warn!(
                                mime_type = %mime_type,
                                error = %err,
                                "SLOPOS_SELECTION_TARGET_DISCONNECTED"
                            );
                        } else {
                            tracing::debug!(
                                mime_type = %mime_type,
                                error = %err,
                                "selection send write failed"
                            );
                        }
                    }
                }
                // Dropping `file` closes the fd → EOF for the receiving client.
                let _ = file.flush();
            })
        {
            // On spawn failure the closure (and thus `fd`) was dropped → EOF.
            tracing::warn!(error = %err, "failed to spawn selection-send thread; fd closed");
        }
    }

    impl SelectionHandler for SloposCompositor {
        type SelectionUserData = MimePayload;

        fn new_selection(
            &mut self,
            ty: SelectionTarget,
            source: Option<SelectionSource>,
            _seat: Seat<Self>,
        ) {
            let mime_types = source.as_ref().map(|s| s.mime_types()).unwrap_or_default();
            match ty {
                SelectionTarget::Clipboard => {
                    self.clipboard_source = source;
                    if self.clipboard_source.is_none() {
                        self.clipboard_data.clear();
                    }
                    tracing::debug!(?mime_types, "clipboard selection updated");
                }
                SelectionTarget::Primary => {
                    self.primary_source = source;
                    if self.primary_source.is_none() {
                        self.primary_data.clear();
                    }
                    tracing::debug!(?mime_types, "primary selection updated");
                }
            }

            // Bridge Wayland → X11 selection when XWayland WM is live.
            if let Some(xwm) = self.xwm.as_mut() {
                let offered = if mime_types.is_empty() {
                    None
                } else {
                    Some(mime_types)
                };
                if let Err(err) = xwm.new_selection(ty, offered) {
                    tracing::debug!(?err, ?ty, "XWayland new_selection failed");
                }
            }
        }

        fn send_selection(
            &mut self,
            ty: SelectionTarget,
            mime_type: String,
            fd: OwnedFd,
            _seat: Seat<Self>,
            user_data: &Self::SelectionUserData,
        ) {
            // Prefer compositor-owned user_data (server-set selection via set_data_device_selection).
            let from_user = selection_bytes_for_mime_with_text_fallback(user_data, &mime_type)
                .map(|b| b.to_vec());
            let from_store = match ty {
                SelectionTarget::Clipboard => {
                    selection_bytes_for_mime_with_text_fallback(&self.clipboard_data, &mime_type)
                        .map(|b| b.to_vec())
                }
                SelectionTarget::Primary => {
                    selection_bytes_for_mime_with_text_fallback(&self.primary_data, &mime_type)
                        .map(|b| b.to_vec())
                }
            };
            let data = from_user.or(from_store);

            if data.is_none() {
                // Last resort: ask XWayland WM to fill the fd (X11 → Wayland).
                if let Some(xwm) = self.xwm.as_mut() {
                    if let Err(err) =
                        xwm.send_selection(ty, mime_type.clone(), fd, self.loop_handle.clone())
                    {
                        tracing::debug!(?err, "XWayland send_selection failed; EOF");
                    }
                    return;
                }
                tracing::debug!(
                    %mime_type,
                    ?ty,
                    "send_selection: no mime data; closing fd (EOF)"
                );
                drop(fd);
                return;
            }

            tracing::debug!(
                %mime_type,
                ?ty,
                bytes = data.as_ref().map(|d| d.len()).unwrap_or(0),
                "send_selection writing mime data"
            );
            write_selection_fd(mime_type, fd, data);
        }
    }

    impl DataDeviceHandler for SloposCompositor {
        fn data_device_state(&self) -> &DataDeviceState {
            &self.data_device_state
        }
    }

    impl ClientDndGrabHandler for SloposCompositor {
        fn started(
            &mut self,
            _source: Option<WlDataSource>,
            icon: Option<WlSurface>,
            _seat: Seat<Self>,
        ) {
            // Client-initiated DnD: smithay routes offer.receive to the client's
            // WlDataSource directly. We only track the optional drag icon here.
            self.dnd_icon = icon.clone();
            eprintln!("SLOPOS_DND_CLIENT_STARTED");
            if icon.is_some() {
                eprintln!("SLOPOS_DND_ICON_ATTACHED");
            }
            tracing::debug!("client DnD started");
        }

        fn dropped(&mut self, _target: Option<WlSurface>, validated: bool, _seat: Seat<Self>) {
            self.dnd_icon = None;
            eprintln!("SLOPOS_DND_DROPPED validated={validated}");
            tracing::debug!("client DnD dropped");
        }
    }

    impl ServerDndGrabHandler for SloposCompositor {
        fn send(&mut self, mime_type: String, fd: OwnedFd, _seat: Seat<Self>) {
            // Server-initiated DnD: write tracked mime payloads, or EOF if none.
            let data =
                selection_bytes_for_mime_with_text_fallback(&self.server_dnd_data, &mime_type)
                    .map(|b| b.to_vec());
            if data.is_none() {
                tracing::debug!(
                    %mime_type,
                    "ServerDndGrabHandler::send: no tracked source data; EOF"
                );
                drop(fd);
                return;
            }
            tracing::debug!(
                %mime_type,
                bytes = data.as_ref().map(|d| d.len()).unwrap_or(0),
                "ServerDndGrabHandler::send writing mime data"
            );
            write_selection_fd(mime_type, fd, data);
        }

        fn cancelled(&mut self, _seat: Seat<Self>) {
            self.server_dnd_data.clear();
        }

        fn finished(&mut self, _seat: Seat<Self>) {
            self.server_dnd_data.clear();
        }
    }

    smithay::delegate_data_device!(SloposCompositor);

    impl PrimarySelectionHandler for SloposCompositor {
        fn primary_selection_state(&self) -> &PrimarySelectionState {
            &self.primary_selection_state
        }
    }

    delegate_primary_selection!(SloposCompositor);

    // -----------------------------------------------------------------------
    // XdgShellHandler
    // -----------------------------------------------------------------------

    impl XdgShellHandler for SloposCompositor {
        fn xdg_shell_state(&mut self) -> &mut XdgShellState {
            &mut self.xdg_shell_state
        }

        fn new_toplevel(&mut self, surface: ToplevelSurface) {
            // Cascade new windows
            let offset = self.next_window_offset;
            self.next_window_offset = next_cascade_offset(offset);
            let (x, y) = cascade_position(offset);
            let requested_geometry = WindowGeometry::new(x, y, DEFAULT_WINDOW_W, DEFAULT_WINDOW_H);
            let output_index =
                output_index_for_geometry(&self.laid_out_outputs, requested_geometry).unwrap_or(0);
            let geometry = clamp_window_to_work_area(
                requested_geometry,
                self.work_area_for_output_index(output_index),
            );
            surface.with_pending_state(|state| {
                // The compositor owns the logical work area, including scale
                // and layer-shell exclusive zones. Do not let a default
                // client request cover the Dock on a small logical output.
                state.size = Some(Size::from((geometry.width, geometry.height)));
                state.states.set(xdg_toplevel::State::Activated);
            });
            surface.send_configure();
            let position = Point::from((geometry.x, geometry.y));
            let mapped_surface = surface.wl_surface().clone();

            let (title, app_id) = with_states(surface.wl_surface(), |states| {
                let data = states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .map(|d| d.lock().unwrap());
                let title = data
                    .as_ref()
                    .and_then(|d| d.title.clone())
                    .unwrap_or_else(|| "Untitled".into());
                let app_id = data
                    .as_ref()
                    .and_then(|d| d.app_id.clone())
                    .unwrap_or_else(|| "slopos-i.app".into());
                (title, app_id)
            });
            let foreign = self
                .foreign_toplevel_list
                .new_toplevel::<SloposCompositor>(&title, &app_id);

            eprintln!(
                "[slopos-compositor] surface mapped at ({},{}) title={title}",
                position.x, position.y
            );

            let window_id = foreign.identifier();
            self.windows.push(MappedWindow {
                toplevel: surface,
                foreign,
                window_id: window_id.clone(),
                app_id: app_id.clone(),
                position,
                size: Size::from((geometry.width, geometry.height)),
                presentation_state: WindowPresentationState::Normal,
                restore_state: None,
                minimized: false,
            });
            // New maps land on the active compositor-owned Space.
            if let Err(error) = self
                .spaces
                .assign_window_for_application(window_id.clone(), &app_id)
            {
                tracing::warn!(%error, %window_id, "could not assign mapped window to active Space");
            }
            self.sync_legacy_workspace_state();
            self.publish_spaces_state(true);
            eprintln!(
                "[slopos-compositor] assign window_id={window_id} active_space={}",
                self.spaces.active_space()
            );
            self.request_full_redraw();

            // Focus the new window and publish accurate wl_surface output membership.
            let idx = self.windows.len() - 1;
            self.sync_surface_output_membership(&mapped_surface, geometry);
            self.focus_window(idx);
        }

        fn move_request(
            &mut self,
            surface: ToplevelSurface,
            seat: wl_seat::WlSeat,
            serial: Serial,
        ) {
            self.begin_interactive_grab(&surface, InteractiveGrabKind::Move, &seat, serial);
        }

        fn resize_request(
            &mut self,
            surface: ToplevelSurface,
            seat: wl_seat::WlSeat,
            serial: Serial,
            edges: xdg_toplevel::ResizeEdge,
        ) {
            let edges = match edges {
                xdg_toplevel::ResizeEdge::Top => ResizeEdges::TOP,
                xdg_toplevel::ResizeEdge::Bottom => ResizeEdges::BOTTOM,
                xdg_toplevel::ResizeEdge::Left => ResizeEdges::LEFT,
                xdg_toplevel::ResizeEdge::Right => ResizeEdges::RIGHT,
                xdg_toplevel::ResizeEdge::TopLeft => ResizeEdges::TOP_LEFT,
                xdg_toplevel::ResizeEdge::TopRight => ResizeEdges::TOP_RIGHT,
                xdg_toplevel::ResizeEdge::BottomLeft => ResizeEdges::BOTTOM_LEFT,
                xdg_toplevel::ResizeEdge::BottomRight => ResizeEdges::BOTTOM_RIGHT,
                _ => return,
            };
            self.begin_interactive_grab(
                &surface,
                InteractiveGrabKind::Resize(edges),
                &seat,
                serial,
            );
        }

        fn maximize_request(&mut self, surface: ToplevelSurface) {
            self.set_window_presentation_state(&surface, WindowPresentationState::Filled);
        }

        fn unmaximize_request(&mut self, surface: ToplevelSurface) {
            self.set_window_presentation_state(&surface, WindowPresentationState::Normal);
        }

        fn fullscreen_request(
            &mut self,
            surface: ToplevelSurface,
            _output: Option<wl_output::WlOutput>,
        ) {
            self.set_window_presentation_state(&surface, WindowPresentationState::Fullscreen);
        }

        fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
            self.set_window_presentation_state(&surface, WindowPresentationState::Normal);
        }

        fn minimize_request(&mut self, surface: ToplevelSurface) {
            let Some(idx) = self
                .windows
                .iter()
                .position(|window| window.toplevel.wl_surface() == surface.wl_surface())
            else {
                return;
            };
            let window_id = self.windows[idx].window_id.clone();
            self.set_window_presentation_state(&surface, WindowPresentationState::Minimized);
            self.windows[idx].minimized = true;
            self.last_minimized_window_id = Some(window_id);
            self.request_full_redraw();
            self.apply_focus_after_workspace_switch();
        }

        fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
            let destroyed_surface = surface.wl_surface();
            let destroys_grab = self.interactive_grab.as_ref().is_some_and(|grab| {
                self.windows.iter().any(|window| {
                    window.window_id == grab.window_id
                        && window.toplevel.wl_surface() == destroyed_surface
                })
            });
            if destroys_grab {
                self.cancel_interactive_grab();
            } else if self.last_pointer_press.as_ref().is_some_and(|press| {
                self.windows.iter().any(|window| {
                    window.window_id == press.window_id
                        && window.toplevel.wl_surface() == destroyed_surface
                })
            }) {
                self.last_pointer_press = None;
                self.left_button_down = false;
            }
            if let Some(idx) = self
                .windows
                .iter()
                .position(|w| w.toplevel.wl_surface() == destroyed_surface)
            {
                let win = self.windows.remove(idx);
                self.spaces.remove_window(&win.window_id);
                self.sync_legacy_workspace_state();
                self.publish_spaces_state(true);
                if self.last_minimized_window_id.as_deref() == Some(win.window_id.as_str()) {
                    self.last_minimized_window_id = None;
                }
                win.foreign.send_closed();
            }
            // Focus topmost **visible** remaining window (not hidden by workspace).
            self.apply_focus_after_workspace_switch();
        }

        fn title_changed(&mut self, surface: ToplevelSurface) {
            let title = with_states(surface.wl_surface(), |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|d| d.lock().unwrap().title.clone())
                    .unwrap_or_default()
            });
            if let Some(w) = self
                .windows
                .iter()
                .find(|w| w.toplevel.wl_surface() == surface.wl_surface())
            {
                w.foreign.send_title(&title);
                w.foreign.send_done();
            }
        }

        fn app_id_changed(&mut self, surface: ToplevelSurface) {
            let app_id = with_states(surface.wl_surface(), |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|d| d.lock().unwrap().app_id.clone())
                    .unwrap_or_default()
            });
            let active_window_id = self.activated_window_id.clone();
            let Some((window_id, is_active)) = self
                .windows
                .iter()
                .find(|w| w.toplevel.wl_surface() == surface.wl_surface())
                .map(|w| {
                    (
                        w.window_id.clone(),
                        active_window_id.as_ref() == Some(&w.window_id),
                    )
                })
            else {
                return;
            };
            let before = self.spaces.window_spaces(&window_id);
            if let Some(w) = self.windows.iter_mut().find(|w| w.window_id == window_id) {
                w.app_id = app_id.clone();
                w.foreign.send_app_id(&app_id);
                w.foreign.send_done();
            }
            if let Err(error) = self
                .spaces
                .assign_window_for_application(window_id.clone(), &app_id)
            {
                tracing::warn!(%error, %app_id, %window_id, "could not apply changed app ID Spaces policy");
            } else if self.spaces.window_spaces(&window_id) != before {
                self.sync_legacy_workspace_state();
                self.publish_spaces_state(true);
                self.request_full_redraw();
                self.apply_focus_after_workspace_switch();
            }
            if is_active {
                if let Err(err) = slopos_compositor::publish_active_toplevel(Some(&app_id)) {
                    tracing::debug!(
                        error = %err,
                        app_id = %app_id,
                        "could not refresh active application"
                    );
                }
            }
        }

        fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
            let popup = PopupKind::from(surface.clone());
            if let Err(err) = self.popup_manager.track_popup(popup.clone()) {
                tracing::debug!(?err, "failed to track xdg popup");
                return;
            }
            let root_ready = find_popup_root_surface(&popup).is_ok();
            let geometry = self.constrained_popup_geometry(&popup, positioner);
            surface.with_pending_state(|state| {
                state.positioner = positioner;
                state.geometry = geometry;
            });
            if root_ready {
                if let Err(err) = surface.send_configure() {
                    tracing::debug!(?err, "failed to configure xdg popup");
                }
            } else {
                tracing::debug!(
                    "deferring parentless popup configure until layer-shell association"
                );
            }
            self.request_redraw();
        }

        fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: WlSerial) {
            self.begin_popup_grab(surface, seat, serial);
        }

        fn reposition_request(
            &mut self,
            surface: PopupSurface,
            positioner: PositionerState,
            token: u32,
        ) {
            let popup = PopupKind::from(surface.clone());
            let geometry = self.constrained_popup_geometry(&popup, positioner);
            surface.with_pending_state(|state| {
                state.positioner = positioner;
                state.geometry = geometry;
            });
            let _serial = surface.send_repositioned(token);
            self.request_redraw();
        }
    }

    delegate_xdg_shell!(SloposCompositor);

    // -----------------------------------------------------------------------
    // Layer shell (menu bar / dock / notifications chrome)
    // -----------------------------------------------------------------------

    impl WlrLayerShellHandler for SloposCompositor {
        fn shell_state(&mut self) -> &mut WlrLayerShellState {
            &mut self.layer_shell_state
        }

        fn new_layer_surface(
            &mut self,
            surface: LayerSurface,
            requested_output: Option<wl_output::WlOutput>,
            layer: Layer,
            namespace: String,
        ) {
            let output_index = self.output_index_for_resource(requested_output.as_ref());
            let output_area = self.output_area_for_index(output_index);
            let output_size = Size::<i32, Logical>::from((output_area.width, output_area.height));
            eprintln!(
                "[slopos-compositor] layer-shell surface namespace={namespace} layer={layer:?} output={} index={output_index}",
                self.output_names
                    .get(output_index)
                    .map(String::as_str)
                    .unwrap_or("unknown")
            );
            let (requested, anchor, margins, exclusive_zone) = layer_surface_request(&surface);
            let local_geo =
                layer_geometry_for(&namespace, layer, output_size, requested, anchor, margins);
            let geo = Rectangle::new(
                Point::from((
                    output_area.x.saturating_add(local_geo.loc.x),
                    output_area.y.saturating_add(local_geo.loc.y),
                )),
                local_geo.size,
            );
            surface.with_pending_state(|state| {
                state.size = Some(geo.size);
            });
            let configure_serial = u32::from(surface.send_configure());
            let wl_surface = surface.wl_surface().clone();
            self.layer_surfaces.push(MappedLayer {
                surface,
                layer,
                namespace,
                output_index,
                geo,
                exclusive_zone,
                requested,
                configure_serial,
                ack_serial: None,
                has_committed: false,
                committed_frame_revision: 0,
            });
            self.sync_surface_to_output(&wl_surface, output_index);
            self.clamp_normal_windows_to_work_area();
            self.request_full_redraw();
        }

        fn new_popup(&mut self, _parent: LayerSurface, surface: PopupSurface) {
            let popup = PopupKind::from(surface.clone());
            let positioner = surface.with_pending_state(|state| state.positioner);
            let geometry = self.constrained_popup_geometry(&popup, positioner);
            surface.with_pending_state(|state| {
                state.positioner = positioner;
                state.geometry = geometry;
            });
            if let Err(err) = surface.send_configure() {
                tracing::debug!(?err, "failed to configure layer-shell popup");
            }
            self.request_redraw();
        }

        fn layer_destroyed(&mut self, surface: LayerSurface) {
            let was_spaces_focused = self
                .seat
                .get_keyboard()
                .and_then(|keyboard| keyboard.current_focus())
                .is_some_and(|focused| focused == surface.wl_surface().clone());
            for output in &self.outputs {
                output.leave(surface.wl_surface());
            }
            self.layer_surfaces
                .retain(|l| l.surface.wl_surface() != surface.wl_surface());
            self.clamp_normal_windows_to_work_area();
            if was_spaces_focused {
                self.apply_focus_after_workspace_switch();
            }
            self.request_full_redraw();
        }
    }

    delegate_layer_shell!(SloposCompositor);

    // -----------------------------------------------------------------------
    // Foreign toplevel list (task list / overview / Force Quit)
    // -----------------------------------------------------------------------

    impl ForeignToplevelListHandler for SloposCompositor {
        fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
            &mut self.foreign_toplevel_list
        }
    }

    delegate_foreign_toplevel_list!(SloposCompositor);

    // -----------------------------------------------------------------------
    // xdg-decoration (server-side preference for external apps)
    // -----------------------------------------------------------------------

    impl XdgDecorationHandler for SloposCompositor {
        fn new_decoration(&mut self, toplevel: ToplevelSurface) {
            use slopos_compositor::{decoration_preference_for_app_id, DecorationPreference};
            use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
            let app_id = with_states(toplevel.wl_surface(), |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|d| d.lock().unwrap().app_id.clone())
                    .unwrap_or_default()
            });
            let mode = match decoration_preference_for_app_id(&app_id) {
                DecorationPreference::ServerSide => Mode::ServerSide,
                DecorationPreference::ClientSide => Mode::ClientSide,
            };
            toplevel.with_pending_state(|state| {
                state.decoration_mode = Some(mode);
            });
            toplevel.send_configure();
        }

        fn request_mode(
            &mut self,
            toplevel: ToplevelSurface,
            mode: smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
        ) {
            toplevel.with_pending_state(|state| {
                state.decoration_mode = Some(mode);
            });
            toplevel.send_configure();
        }

        fn unset_mode(&mut self, toplevel: ToplevelSurface) {
            use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
            // Prefer server-side for unknown clients when unset.
            toplevel.with_pending_state(|state| {
                state.decoration_mode = Some(Mode::ServerSide);
            });
            toplevel.send_configure();
        }
    }

    smithay::delegate_xdg_decoration!(SloposCompositor);

    // text-input-v3 manager (global advertised when policy enables it)
    smithay::delegate_text_input_manager!(SloposCompositor);

    // input-method-v2 (paired with text-input for IME clients)
    impl smithay::wayland::input_method::InputMethodHandler for SloposCompositor {
        fn new_popup(&mut self, surface: smithay::wayland::input_method::PopupSurface) {
            tracing::debug!("input-method popup created");
            self.im_popups.push(surface);
        }

        fn dismiss_popup(&mut self, surface: smithay::wayland::input_method::PopupSurface) {
            self.im_popups.retain(|p| p != &surface);
        }

        fn popup_repositioned(&mut self, _surface: smithay::wayland::input_method::PopupSurface) {}

        fn parent_geometry(&self, parent: &WlSurface) -> smithay::utils::Rectangle<i32, Logical> {
            // Use focused window geometry when the parent matches a toplevel.
            for w in &self.windows {
                if w.toplevel.wl_surface() == parent {
                    return smithay::utils::Rectangle::new(w.position, w.size);
                }
            }
            smithay::utils::Rectangle::default()
        }
    }

    smithay::delegate_input_method_manager!(SloposCompositor);

    // -----------------------------------------------------------------------
    // OutputHandler (required by delegate_output!)
    // -----------------------------------------------------------------------

    impl OutputHandler for SloposCompositor {}

    delegate_output!(SloposCompositor);

    // -----------------------------------------------------------------------
    // XWayland (P1.3) — best-effort under nested X11
    //
    // Nested under Xvfb/X11 the compositor already owns DISPLAY. XWayland is
    // still spawned (own display number) so the code path exists and X clients
    // can attach when the binary + runtime allow it. Rootless X11 windows are
    // tracked in the same compositor scene lifecycle as native surface trees.
    // -----------------------------------------------------------------------

    impl XWaylandShellHandler for SloposCompositor {
        fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
            &mut self.xwayland_shell_state
        }

        fn surface_associated(
            &mut self,
            _xwm: XwmId,
            wl_surface: WlSurface,
            surface: X11WmSurface,
        ) {
            tracing::info!(
                title = %surface.title(),
                "XWayland surface associated with wl_surface"
            );
            let window_id = surface.window_id();
            let should_focus = !surface.is_override_redirect();
            self.x11_scene.associate(surface, wl_surface.clone());
            self.ensure_x11_space_membership(window_id);
            self.sync_x11_scene_output_membership(window_id);
            self.note_x11_surface_mapped(window_id);
            if should_focus {
                self.focus_surface(Some(wl_surface));
            }
            self.request_full_redraw();
        }
    }

    delegate_xwayland_shell!(SloposCompositor);

    impl XwmHandler for SloposCompositor {
        fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
            self.xwm.as_mut().expect("X11Wm missing for XwmHandler")
        }

        fn new_window(&mut self, _xwm: XwmId, window: X11WmSurface) {
            tracing::debug!(title = %window.title(), "X11 new_window");
            self.x11_scene.register(window);
        }

        fn new_override_redirect_window(&mut self, _xwm: XwmId, window: X11WmSurface) {
            tracing::debug!(title = %window.title(), "X11 override-redirect window");
            self.x11_scene.register(window);
        }

        fn map_window_request(&mut self, _xwm: XwmId, window: X11WmSurface) {
            // Grant map so X clients don't hang waiting for the WM.
            if let Err(err) = window.set_mapped(true) {
                tracing::debug!(?err, "X11 set_mapped failed");
            }
            let geo = window.geometry();
            if let Err(err) = window.configure(Some(geo)) {
                tracing::debug!(?err, "X11 configure failed");
            }
            let window_id = window.window_id();
            self.ensure_x11_space_membership(window_id);
            self.x11_scene.set_mapped(window.clone(), true);
            self.sync_x11_scene_output_membership(window_id);
            self.note_x11_surface_mapped(window_id);
            self.request_full_redraw();
            tracing::info!(title = %window.title(), "X11 map_window_request granted");
        }

        fn map_window_notify(&mut self, _xwm: XwmId, window: X11WmSurface) {
            let window_id = window.window_id();
            self.ensure_x11_space_membership(window_id);
            self.x11_scene.set_mapped(window.clone(), true);
            self.x11_scene.configure(window.clone(), window.geometry());
            self.sync_x11_scene_output_membership(window_id);
            self.note_x11_surface_mapped(window_id);
            self.request_full_redraw();
        }

        fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11WmSurface) {
            let window_id = window.window_id();
            self.ensure_x11_space_membership(window_id);
            self.x11_scene.set_mapped(window.clone(), true);
            self.sync_x11_scene_output_membership(window_id);
            self.note_x11_surface_mapped(window_id);
            self.request_full_redraw();
            tracing::debug!(title = %window.title(), "X11 override-redirect mapped");
        }

        fn unmapped_window(&mut self, _xwm: XwmId, window: X11WmSurface) {
            let window_id = window.window_id();
            let was_focused = self.x11_surface_has_keyboard_focus(window_id);
            self.x11_scene.unmap(window_id);
            if was_focused {
                self.x11_scene.set_active(None);
            }
            self.leave_x11_scene_output_membership(window_id);
            if was_focused {
                self.apply_focus_after_workspace_switch();
            }
            tracing::info!(window = window_id, "XWayland surface unmapped from scene");
            self.request_full_redraw();
        }

        fn destroyed_window(&mut self, _xwm: XwmId, window: X11WmSurface) {
            let window_id = window.window_id();
            let was_focused = self.x11_surface_has_keyboard_focus(window_id);
            self.leave_x11_scene_output_membership(window_id);
            self.x11_scene.destroy(window_id);
            self.remove_x11_space_membership(window_id);
            if was_focused {
                self.x11_scene.set_active(None);
                self.apply_focus_after_workspace_switch();
            }
            tracing::info!(window = window_id, "XWayland surface destroyed from scene");
            self.request_full_redraw();
        }

        fn configure_request(
            &mut self,
            _xwm: XwmId,
            window: X11WmSurface,
            x: Option<i32>,
            y: Option<i32>,
            w: Option<u32>,
            h: Option<u32>,
            _reorder: Option<Reorder>,
        ) {
            let mut geo = window.geometry();
            if let Some(x) = x {
                geo.loc.x = x;
            }
            if let Some(y) = y {
                geo.loc.y = y;
            }
            if let Some(w) = w {
                geo.size.w = w as i32;
            }
            if let Some(h) = h {
                geo.size.h = h as i32;
            }
            if let Err(err) = window.configure(Some(geo)) {
                tracing::debug!(?err, "X11 configure request failed");
            }
            let window_id = window.window_id();
            self.x11_scene.configure(window, geo);
            self.sync_x11_scene_output_membership(window_id);
            tracing::info!(window = window_id, "XWayland surface configured in scene");
            self.request_full_redraw();
        }

        fn configure_notify(
            &mut self,
            _xwm: XwmId,
            window: X11WmSurface,
            geometry: Rectangle<i32, Logical>,
            _above: Option<X11Window>,
        ) {
            let window_id = window.window_id();
            self.x11_scene.configure(window, geometry);
            self.sync_x11_scene_output_membership(window_id);
            tracing::info!(window = window_id, "XWayland surface configured in scene");
            self.request_full_redraw();
        }

        fn resize_request(
            &mut self,
            _xwm: XwmId,
            window: X11WmSurface,
            button: u32,
            resize_edge: ResizeEdge,
        ) {
            let edges = x11_resize_edge_to_resize_edges(resize_edge);
            self.begin_x11_interactive_grab(&window, InteractiveGrabKind::Resize(edges), button);
        }

        fn move_request(&mut self, _xwm: XwmId, window: X11WmSurface, button: u32) {
            self.begin_x11_interactive_grab(&window, InteractiveGrabKind::Move, button);
        }

        fn allow_selection_access(&mut self, _xwm: XwmId, _selection: SelectionTarget) -> bool {
            // Allow X clients to read the Wayland selection store.
            true
        }

        fn send_selection(
            &mut self,
            _xwm: XwmId,
            selection: SelectionTarget,
            mime_type: String,
            fd: OwnedFd,
        ) {
            let store = match selection {
                SelectionTarget::Clipboard => &self.clipboard_data,
                SelectionTarget::Primary => &self.primary_data,
            };
            let data =
                selection_bytes_for_mime_with_text_fallback(store, &mime_type).map(|b| b.to_vec());
            write_selection_fd(mime_type, fd, data);
        }

        fn new_selection(
            &mut self,
            _xwm: XwmId,
            selection: SelectionTarget,
            mime_types: Vec<String>,
        ) {
            tracing::debug!(?selection, ?mime_types, "X11 client set selection");
        }

        fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
            match selection {
                SelectionTarget::Clipboard => self.clipboard_data.clear(),
                SelectionTarget::Primary => self.primary_data.clear(),
            }
        }

        fn disconnected(&mut self, _xwm: XwmId) {
            tracing::warn!("XWayland WM disconnected");
            if self.x11_interactive_grab.is_some() {
                self.cancel_interactive_grab();
            }
            self.xwayland_keyboard_focus = None;
            self.xwm = None;
            self.xdisplay = None;
            self.xwayland_client_id = None;
            let entries = self.x11_scene.clear();
            for entry in entries {
                self.remove_x11_space_membership(entry.surface.window_id());
                if let Some(surface) = entry.wl_surface {
                    for output in &self.outputs {
                        output.leave(&surface);
                    }
                }
            }
            self.request_full_redraw();
            std::env::remove_var("SLOPOS_XWAYLAND_DISPLAY");

            if self.xwayland_recovery_budget.take_restart() {
                tracing::warn!(
                    remaining = self.xwayland_recovery_budget.remaining(),
                    "XWayland WM disconnected; restarting XWayland"
                );
                try_start_xwayland(self);
            } else {
                tracing::error!(
                    "XWayland recovery budget exhausted; entering terminal disconnected state"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Input dispatch helpers (called from the X11 event handler)
    // -----------------------------------------------------------------------

    fn handle_keyboard_event<E>(state: &mut SloposCompositor, ev: &E)
    where
        E: KeyboardKeyEvent<X11Input>,
    {
        use smithay::backend::input::KeyState;
        use smithay::input::keyboard::Keysym;

        let serial = state.next_serial();
        let time = ev.time_msec();
        let keycode = ev.key_code();
        let key_state = ev.state();

        if key_state == KeyState::Pressed {
            let focused_x11 = state
                .seat
                .get_keyboard()
                .and_then(|keyboard| keyboard.current_focus())
                .and_then(|surface| state.x11_scene.window_for_surface(&surface));
            if let Some(window) = focused_x11 {
                tracing::info!(
                    window = window.window_id(),
                    "XWayland keyboard focus delivered"
                );
            }
        }

        if let Some(kb) = state.seat.get_keyboard() {
            kb.input::<(), _>(
                state,
                keycode,
                key_state,
                serial,
                time,
                |data, mods, keysym| {
                    // Super+Right / Super+Left: cycle virtual workspaces (live filter).
                    if key_state == KeyState::Pressed && mods.logo {
                        let sym = keysym.modified_sym();
                        if sym == Keysym::o || sym == Keysym::O {
                            slopos_compositor::client_spawn::spawn_client(
                                &data.wayland_socket_name,
                                "finder",
                            );
                            return FilterResult::Intercept(());
                        }
                        if sym == Keysym::l || sym == Keysym::L {
                            slopos_compositor::client_spawn::spawn_client(
                                &data.wayland_socket_name,
                                "slopos-lock",
                            );
                            return FilterResult::Intercept(());
                        }
                        if sym == Keysym::Right || sym == Keysym::Page_Down {
                            data.cycle_workspace_next();
                            return FilterResult::Intercept(());
                        }
                        if sym == Keysym::Left || sym == Keysym::Page_Up {
                            data.cycle_workspace_prev();
                            return FilterResult::Intercept(());
                        }
                        // Super+1..8 → activate workspace 0..7
                        let digit = match sym {
                            Keysym::_1 => Some(0u8),
                            Keysym::_2 => Some(1),
                            Keysym::_3 => Some(2),
                            Keysym::_4 => Some(3),
                            Keysym::_5 => Some(4),
                            Keysym::_6 => Some(5),
                            Keysym::_7 => Some(6),
                            Keysym::_8 => Some(7),
                            _ => None,
                        };
                        if let Some(i) = digit {
                            data.activate_workspace_index(i);
                            return FilterResult::Intercept(());
                        }
                    }
                    FilterResult::Forward
                },
            );
        }
    }

    fn maybe_activate_pointer_constraint(
        state: &SloposCompositor,
        pointer: &PointerHandle<SloposCompositor>,
    ) {
        let location = state.pointer_pos;
        let Some((surface, surface_location)) = state.surface_under(location) else {
            return;
        };
        if pointer.current_focus().as_ref() != Some(&surface) {
            return;
        }
        with_pointer_constraint(&surface, pointer, |constraint| {
            let Some(constraint) = constraint else {
                return;
            };
            if constraint.is_active() {
                return;
            }
            let local = (location - surface_location).to_i32_round();
            if constraint
                .region()
                .is_none_or(|region| region.contains(local))
            {
                constraint.activate();
            }
        });
    }

    #[derive(Clone, Copy)]
    struct AxisFrameInput {
        time: u32,
        source: AxisSource,
        directions: (AxisRelativeDirection, AxisRelativeDirection),
        amounts: (Option<f64>, Option<f64>),
        v120: (Option<f64>, Option<f64>),
    }

    fn build_axis_frame(input: AxisFrameInput) -> AxisFrame {
        let AxisFrameInput {
            time,
            source,
            directions: (horizontal_direction, vertical_direction),
            amounts: (horizontal_amount, vertical_amount),
            v120: (horizontal_v120, vertical_v120),
        } = input;
        let mut frame = AxisFrame::new(time)
            .source(source)
            .relative_direction(Axis::Horizontal, horizontal_direction)
            .relative_direction(Axis::Vertical, vertical_direction);

        if let Some(amount) = horizontal_amount {
            frame = frame.value(Axis::Horizontal, amount);
        }
        if let Some(amount) = vertical_amount {
            frame = frame.value(Axis::Vertical, amount);
        }
        if let Some(steps) = horizontal_v120 {
            frame = frame.v120(Axis::Horizontal, steps.round() as i32);
        }
        if let Some(steps) = vertical_v120 {
            frame = frame.v120(Axis::Vertical, steps.round() as i32);
        }

        frame
    }

    fn axis_frame_from_event<E>(ev: &E) -> AxisFrame
    where
        E: PointerAxisEvent<X11Input>,
    {
        build_axis_frame(AxisFrameInput {
            time: ev.time_msec(),
            source: ev.source(),
            directions: (
                ev.relative_direction(Axis::Horizontal),
                ev.relative_direction(Axis::Vertical),
            ),
            amounts: (ev.amount(Axis::Horizontal), ev.amount(Axis::Vertical)),
            v120: (
                ev.amount_v120(Axis::Horizontal),
                ev.amount_v120(Axis::Vertical),
            ),
        })
    }

    fn handle_pointer_axis<E>(state: &mut SloposCompositor, ev: &E)
    where
        E: PointerAxisEvent<X11Input>,
    {
        let frame = axis_frame_from_event(ev);
        if let Some(ptr) = state.seat.get_pointer() {
            // Axis events follow the pointer's current focus/grab. Do not
            // retarget it here: motion and button paths own focus updates.
            ptr.axis(state, frame);
            ptr.frame(state);
        }
        state.request_redraw();
    }

    fn constrain_pointer_destination(
        state: &SloposCompositor,
        pointer: &PointerHandle<SloposCompositor>,
        current: Point<f64, Logical>,
        desired: Point<f64, Logical>,
    ) -> Point<f64, Logical> {
        let Some((surface, surface_location)) = state.surface_under(current) else {
            return desired;
        };

        let mut mode = PointerConstraintMotion::Free;
        let mut region = None;
        with_pointer_constraint(&surface, pointer, |constraint| {
            let Some(constraint) = constraint else {
                return;
            };
            if !constraint.is_active() {
                return;
            }
            let current_local = (current - surface_location).to_i32_round();
            if !constraint
                .region()
                .is_none_or(|candidate| candidate.contains(current_local))
            {
                return;
            }
            mode = match &*constraint {
                PointerConstraint::Locked(_) => PointerConstraintMotion::Locked,
                PointerConstraint::Confined(_) => PointerConstraintMotion::Confined,
            };
            region = constraint.region().cloned();
        });

        if mode == PointerConstraintMotion::Free {
            return desired;
        }
        if mode == PointerConstraintMotion::Locked {
            return current;
        }

        let delta = desired - current;
        let x_target = current + Point::from((delta.x, 0.0));
        let y_target = current + Point::from((0.0, delta.y));
        let same_surface = |target: Point<f64, Logical>| {
            state
                .surface_under(target)
                .is_some_and(|(candidate, _)| candidate == surface)
        };
        let inside_region = |target: Point<f64, Logical>| {
            region.as_ref().is_none_or(|candidate| {
                candidate.contains((target - surface_location).to_i32_round())
            })
        };
        let resolved = slopos_compositor::pointer_policy::resolve_pointer_delta(
            mode,
            (delta.x, delta.y),
            same_surface(x_target) && inside_region(x_target),
            same_surface(y_target) && inside_region(y_target),
        );
        let candidate = current + Point::from(resolved);
        if same_surface(candidate) && inside_region(candidate) {
            candidate
        } else {
            current
        }
    }

    fn handle_pointer_motion<E>(state: &mut SloposCompositor, ev: &E)
    where
        E: PointerMotionAbsoluteEvent<X11Input>,
    {
        let logical = Size::<i32, Logical>::from((state.output_size.w, state.output_size.h));
        let raw_pos = ev.position_transformed(logical);
        let previous_raw = state
            .last_backend_pointer_pos
            .replace(raw_pos)
            .unwrap_or(raw_pos);
        let delta = Point::from((raw_pos.x - previous_raw.x, raw_pos.y - previous_raw.y));
        let current = state.pointer_pos;
        let serial = state.next_serial();
        let time = ev.time_msec();

        if let Some(ptr) = state.seat.get_pointer() {
            // Relative motion follows the raw host samples even while the visible
            // pointer is locked by zwp_pointer_constraints_v1.
            let relative_focus = state.surface_under(current);
            ptr.relative_motion(
                state,
                relative_focus,
                &RelativeMotionEvent {
                    delta,
                    delta_unaccel: delta,
                    utime: u64::from(time) * 1_000,
                },
            );

            let pos = constrain_pointer_destination(state, &ptr, current, raw_pos);
            state.pointer_pos = pos;
            state.request_redraw();
            let focus = state.surface_under(pos);
            ptr.motion(
                state,
                focus,
                &MotionEvent {
                    location: pos,
                    serial,
                    time,
                },
            );
            ptr.frame(state);
            maybe_activate_pointer_constraint(state, &ptr);
        } else {
            state.pointer_pos = raw_pos;
            state.request_redraw();
        }
    }

    fn handle_pointer_button<E>(state: &mut SloposCompositor, ev: &E)
    where
        E: PointerButtonEvent<X11Input>,
    {
        let serial = state.next_serial();
        let time = ev.time_msec();
        let button = ev.button_code();
        let btn_state = ev.state();

        let primary_button = button == 0x110 || button == 1;
        if primary_button {
            state.left_button_down = btn_state == ButtonState::Pressed;
        }

        // On press: hit-test surfaces and focus the topmost one.
        if btn_state == ButtonState::Pressed {
            let pos = state.pointer_pos;
            let hit = state.surface_under(pos);
            if let Some((surface, _)) = hit.as_ref() {
                if let Some(window) = state.x11_scene.window_for_surface(surface) {
                    tracing::info!(
                        window = window.window_id(),
                        x = pos.x,
                        y = pos.y,
                        "XWayland surface hit-tested for input"
                    );
                }
            }
            let mapped_window_index = hit
                .as_ref()
                .and_then(|(surface, _)| state.mapped_window_index_for_surface(surface));
            let x11_window = hit
                .as_ref()
                .and_then(|(surface, _)| state.x11_scene.window_for_surface(surface));
            if primary_button {
                state.last_pointer_press = if let Some(index) = mapped_window_index {
                    Some(PointerPress {
                        serial,
                        window_id: state.windows[index].window_id.clone(),
                        x11_window_id: None,
                    })
                } else {
                    x11_window
                        .as_ref()
                        .filter(|window| !window.is_override_redirect())
                        .map(|window| PointerPress {
                            serial,
                            window_id: String::new(),
                            x11_window_id: Some(window.window_id()),
                        })
                };
            }
            match hit {
                Some((surface, _)) => match mapped_window_index {
                    Some(idx) => {
                        state.focus_window(idx);
                    }
                    None if x11_window
                        .as_ref()
                        .is_some_and(|window| window.is_override_redirect()) =>
                    {
                        tracing::debug!(
                            window = x11_window
                                .as_ref()
                                .map(X11WmSurface::window_id)
                                .unwrap_or_default(),
                            "override-redirect surface kept out of keyboard focus"
                        );
                    }
                    None => {
                        state.focus_surface(Some(surface));
                    }
                },
                None => state.focus_surface(None),
            }
            // Retarget pointer focus so the button is delivered to the
            // surface under the current click coordinates.
            state.forward_pointer_motion(time);
        }

        if let Some(ptr) = state.seat.get_pointer() {
            ptr.button(
                state,
                &ButtonEvent {
                    serial,
                    time,
                    button,
                    state: btn_state,
                },
            );
            ptr.frame(state);
        }
        if primary_button && btn_state == ButtonState::Released {
            state.finish_interactive_grab();
        }
        state.request_redraw();
    }

    /// Create one or more wl_output globals at the given logical origins.
    ///
    /// `laid_out` positions come from shell `SLOPOS_OUTPUTS_LAYOUT` or from
    /// `SLOPOS_OUTPUTS` + layout mode. `names` are connector names when known
    /// (else synthetic `X11-N`). `scale` is advertised on each output (HiDPI);
    /// mode sizes stay logical width×height; scale is the wl_output scale factor.
    ///
    /// Nested path only places logical outputs — no DRM modeset for external
    /// connectors in this pass.
    fn configure_output(
        output: &Output,
        laid_out: &LaidOutOutput,
        refresh_mhz: i32,
        scale: OutputScale,
    ) {
        let scale_i32 = scale.as_f64().round().max(1.0) as i32;
        let mode = Mode {
            size: (laid_out.config.width, laid_out.config.height).into(),
            refresh: refresh_mhz,
        };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Integer(scale_i32)),
            Some((laid_out.x, laid_out.y).into()),
        );
        output.set_preferred(mode);
    }

    fn create_output(
        display_handle: &DisplayHandle,
        laid_out: &LaidOutOutput,
        name: String,
        index: usize,
        refresh_mhz: i32,
        scale: OutputScale,
    ) -> (Output, GlobalId) {
        let output = Output::new(
            name.clone(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "SLOPOS-I".into(),
                model: format!("Logical Output {}", index + 1),
            },
        );
        configure_output(&output, laid_out, refresh_mhz, scale);
        let global = output.create_global::<SloposCompositor>(display_handle);
        tracing::info!(
            "wl_output {} ({}) {}x{} at ({},{}) refresh={} mHz {}",
            index + 1,
            name,
            laid_out.config.width,
            laid_out.config.height,
            laid_out.x,
            laid_out.y,
            refresh_mhz,
            output_scale_summary(scale)
        );
        (output, global)
    }

    /// Create one or more wl_output globals at the given logical origins.
    fn create_outputs(
        display_handle: &DisplayHandle,
        laid_out: &[LaidOutOutput],
        names: &[String],
        refresh_mhz: i32,
        scale: OutputScale,
    ) -> (Vec<Output>, Vec<GlobalId>, Size<i32, Physical>) {
        let total = total_output_size(laid_out);
        let total_phys = apply_scale_to_output_config(total, scale);
        let mut outputs = Vec::with_capacity(laid_out.len());
        let mut globals = Vec::with_capacity(laid_out.len());
        for (index, laid_out) in laid_out.iter().enumerate() {
            let name = names
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("X11-{}", index + 1));
            let (output, global) =
                create_output(display_handle, laid_out, name, index, refresh_mhz, scale);
            outputs.push(output);
            globals.push(global);
        }
        (
            outputs,
            globals,
            Size::<i32, Physical>::from((total_phys.width, total_phys.height)),
        )
    }

    /// Best-effort XWayland startup. Returns false when the binary is missing or spawn fails.
    ///
    /// Under nested X11 this is still useful: XWayland gets its own display number and
    /// clients can set DISPLAY=:N. Full scene integration of X11 surfaces remains limited
    /// because the compositor itself is an X11 client of the host server.
    fn try_start_xwayland(state: &mut SloposCompositor) {
        state.ensure_xwayland_startup_watchdog();
        // Allow opt-out: SLOPOS_XWAYLAND=0
        if std::env::var("SLOPOS_XWAYLAND")
            .map(|v| matches!(v.as_str(), "0" | "false" | "off" | "no"))
            .unwrap_or(false)
        {
            tracing::info!("XWayland disabled via SLOPOS_XWAYLAND");
            return;
        }

        use std::process::Stdio;

        match XWayland::spawn(
            &state.display_handle,
            None,
            std::iter::empty::<(String, String)>(),
            true,
            Stdio::null(),
            Stdio::null(),
            |_| (),
        ) {
            Ok((xwayland, client)) => {
                state.xwayland_client_id = Some(client.id());
                let display_number_hint = xwayland.display_number();
                tracing::info!(
                    "XWayland spawning (will claim DISPLAY=:{} when ready)",
                    display_number_hint
                );
                let ret = state.loop_handle.insert_source(xwayland, move |event, _, data| {
                    match event {
                        XWaylandEvent::Ready {
                            x11_socket,
                            display_number,
                        } => {
                            tracing::info!(
                                "XWayland ready on DISPLAY=:{} — starting X11 WM",
                                display_number
                            );
                            match X11Wm::start_wm(data.loop_handle.clone(), x11_socket, client.clone())
                            {
                                Ok(wm) => {
                                    data.xwm = Some(wm);
                                    data.xdisplay = Some(display_number);
                                    // Expose DISPLAY for child processes launched later.
                                    std::env::set_var("SLOPOS_XWAYLAND_DISPLAY", format!(":{display_number}"));
                                    eprintln!(
                                        "[slopos-compositor] XWayland ready DISPLAY=:{}",
                                        display_number
                                    );
                                }
                                Err(err) => {
                                    tracing::warn!(?err, "Failed to start X11Wm for XWayland");
                                    data.recover_xwayland_startup();
                                }
                            }
                        }
                        XWaylandEvent::Error => {
                            tracing::warn!(
                                "XWayland failed to start (binary missing, nested X11 conflict, or crash)"
                            );
                            data.recover_xwayland_startup();
                        }
                    }
                });
                if let Err(err) = ret {
                    tracing::warn!(?err, "Failed to insert XWayland event source");
                }
            }
            Err(err) => {
                // Nested X11 or missing XWayland package: route the failure
                // through the same session-scoped watchdog budget as an
                // Error/WM-disconnect event. This prevents a silent
                // pre-Ready failure while keeping retries bounded.
                tracing::warn!(
                    error = %err,
                    "XWayland spawn failed (install `xwayland` package for X11 client support; nested X11 may still be limited)"
                );
                eprintln!(
                    "[slopos-compositor] XWayland unavailable: {err} (continuing without it)"
                );
                state.recover_xwayland_startup();
            }
        }
    }

    #[cfg(test)]
    pub fn parse_bool_env(key: &str) -> bool {
        match std::env::var(key) {
            Ok(v) => matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            Err(_) => false,
        }
    }

    pub(crate) fn default_backend_for_host(
        display: Option<&str>,
        _wayland_display: Option<&str>,
    ) -> &'static str {
        // The nested implementation below is Smithay's X11 backend. A host
        // Wayland socket is not a valid transport for it.
        if display.is_some_and(|value| !value.is_empty()) {
            "nested"
        } else {
            "drm"
        }
    }

    pub(crate) fn validate_nested_transport(
        requested_backend: &str,
        display: Option<&str>,
    ) -> Result<(), String> {
        if matches!(requested_backend, "nested" | "x11" | "winit")
            && display.is_none_or(|value| value.is_empty())
        {
            return Err(
                "nested backend requires a non-empty DISPLAY (nested transport is X11-only); use --backend drm or --backend headless"
                    .to_owned(),
            );
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Entry point
    // -----------------------------------------------------------------------

    pub fn run() -> anyhow::Result<()> {
        tracing_subscriber::fmt::init();

        let args: Vec<String> = std::env::args().collect();
        let mut backend_arg: Option<String> = None;
        let mut idx = 1;
        while idx < args.len() {
            if args[idx] == "--backend" && idx + 1 < args.len() {
                backend_arg = Some(args[idx + 1].clone());
                idx += 2;
            } else if args[idx].starts_with("--backend=") {
                backend_arg = Some(args[idx].trim_start_matches("--backend=").to_string());
                idx += 1;
            } else {
                idx += 1;
            }
        }

        // Backend selection is explicit and fail-fast. The production session
        // never substitutes labwc/sway or silently changes the requested backend.
        let requested_backend = backend_arg.unwrap_or_else(|| {
            default_backend_for_host(
                std::env::var("DISPLAY").ok().as_deref(),
                std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
            )
            .to_owned()
        });

        if let Err(error) =
            validate_nested_transport(&requested_backend, std::env::var("DISPLAY").ok().as_deref())
        {
            anyhow::bail!(error);
        }

        if requested_backend == "drm" {
            eprintln!("[slopos-compositor] backend: SessionDrm (explicit)");
            return slopos_compositor::session_drm::run_drm_session();
        }

        let headless = requested_backend == "headless";
        if !matches!(
            requested_backend.as_str(),
            "nested" | "x11" | "winit" | "headless"
        ) {
            anyhow::bail!(
                "unsupported backend '{requested_backend}'; expected drm, nested, x11, winit, or headless"
            );
        }
        if requested_backend == "winit" {
            tracing::warn!(
                "--backend winit currently uses Smithay's nested X11 transport; use --backend nested"
            );
        }
        let backend_kind = if headless {
            CompositorBackendKind::Headless
        } else {
            CompositorBackendKind::NestedX11
        };
        if !headless {
            // Nested output readback uses the same GL renderer and element list
            // as presentation; headless has no framebuffer to capture.
            slopos_compositor::screenshot::install_signal_handler();
        }
        eprintln!(
            "[slopos-compositor] backend: {} (explicit)",
            if headless { "Headless" } else { "NestedX11" }
        );

        // ---- Display policy (HDR / VRR / refresh / color) ----
        // Nested/headless have software pacing but no verified variable-refresh
        // or HDR scanout path. Keep those capabilities explicit so a runtime
        // request cannot silently fabricate hardware support.
        let vrr_supported = false;
        let mut display_policy = DisplayPolicy::resolve();
        let mut hdr_caps = HdrCapabilities::detect();
        let initial_outcome =
            hdr_caps.negotiate_request(display_policy.hdr_requested, display_policy.color_space);
        let mut display_policy_fallback_reason = match initial_outcome.fallback_reason {
            HdrFallbackReason::None => None,
            HdrFallbackReason::HdrUnsupported => Some("hdr_unsupported".to_string()),
            HdrFallbackReason::RequestedColorSpaceUnsupported => {
                Some("requested_color_space_unsupported".to_string())
            }
            HdrFallbackReason::SdrPolicyForcesSrgb => Some("sdr_policy_forces_srgb".to_string()),
            HdrFallbackReason::NoUsableHdrColorSpace => {
                Some("no_usable_hdr_color_space".to_string())
            }
        };
        if !vrr_supported
            && (display_policy.vrr_adaptive
                || matches!(display_policy.refresh_rate, RefreshRate::Adaptive))
        {
            display_policy.vrr_adaptive = false;
            if matches!(display_policy.refresh_rate, RefreshRate::Adaptive) {
                display_policy.refresh_rate = RefreshRate::Hz60;
            }
            display_policy_fallback_reason = Some("vrr_unsupported".to_string());
        }
        let effective_refresh = display_policy.effective_refresh_rate();
        let frame_scheduler = FrameScheduler::new(effective_refresh);
        let refresh_mhz: i32 = match effective_refresh {
            RefreshRate::Adaptive => 60_000, // advertise 60; pacing is free-run
            r => (r.as_hz() as i32) * 1000,
        };

        let policy_line = display_policy.summary_line(hdr_caps.hdr_supported);
        tracing::info!(
            "display policy applied: {policy_line} color_applied={} fallback={:?}",
            initial_outcome.exact_match,
            display_policy_fallback_reason
        );
        eprintln!("[slopos-compositor] display policy: {policy_line}");
        if display_policy.hdr_requested && !hdr_caps.hdr_supported {
            tracing::info!(
                "HDR requested but not supported under nested X11/no-KMS probe; staying SDR ({})",
                hdr_caps.current_color_space.as_str()
            );
        }

        let mut event_loop: EventLoop<SloposCompositor> = EventLoop::try_new()?;
        let display: Display<SloposCompositor> = Display::new()?;
        let display_handle = display.handle();
        let loop_handle = event_loop.handle();
        let loop_signal = event_loop.get_signal();

        // Protocol states
        let compositor_state = CompositorState::new::<SloposCompositor>(&display_handle);
        let shm_state = ShmState::new::<SloposCompositor>(&display_handle, vec![]);
        let mut seat_state = SeatState::new();
        let relative_pointer_state =
            RelativePointerManagerState::new::<SloposCompositor>(&display_handle);
        let pointer_constraints_state =
            PointerConstraintsState::new::<SloposCompositor>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<SloposCompositor>(&display_handle);
        let data_device_state = DataDeviceState::new::<SloposCompositor>(&display_handle);
        let primary_selection_state =
            PrimarySelectionState::new::<SloposCompositor>(&display_handle);
        let output_manager_state =
            OutputManagerState::new_with_xdg_output::<SloposCompositor>(&display_handle);
        let xwayland_shell_state = XWaylandShellState::new::<SloposCompositor>(&display_handle);
        let layer_shell_state = WlrLayerShellState::new::<SloposCompositor>(&display_handle);
        let foreign_toplevel_list =
            ForeignToplevelListState::new::<SloposCompositor>(&display_handle);
        let xdg_decoration_state = XdgDecorationState::new::<SloposCompositor>(&display_handle);

        // text-input-v3 global when SLOPOS_TEXT_INPUT requests it (default: on)
        // Default "full" advertises text-input-v3 + input-method-v2 for IME clients.
        // Set SLOPOS_TEXT_INPUT=0 to disable, or v3 for text-input only.
        let text_input_cap = text_input_capability_from_env(
            std::env::var("SLOPOS_TEXT_INPUT")
                .ok()
                .as_deref()
                .or(Some("full")),
        );
        let text_input_state = if matches!(
            text_input_cap,
            TextInputCapability::TextInputV3 | TextInputCapability::InputMethodAndTextInput
        ) {
            eprintln!(
                "[slopos-compositor] {}",
                text_input_capability_summary(text_input_cap)
            );
            Some(smithay::wayland::text_input::TextInputManagerState::new::<
                SloposCompositor,
            >(&display_handle))
        } else {
            eprintln!(
                "[slopos-compositor] {}",
                text_input_capability_summary(TextInputCapability::None)
            );
            None
        };
        let input_method_state =
            if matches!(text_input_cap, TextInputCapability::InputMethodAndTextInput) {
                eprintln!("[slopos-compositor] input_method=zwp_input_method_v2");
                Some(
                    smithay::wayland::input_method::InputMethodManagerState::new::<
                        SloposCompositor,
                        _,
                    >(&display_handle, |_client| true),
                )
            } else {
                None
            };

        // Seat: keyboard + pointer
        let mut seat: Seat<SloposCompositor> = seat_state.new_wl_seat(&display_handle, "seat0");
        seat.add_keyboard(XkbConfig::default(), 200, 25)?;
        seat.add_pointer();

        // ---- Outputs (P1.2 multi-output) + HiDPI scale ----
        let output_scale = detect_output_scale_from_env().unwrap_or(OutputScale::IDENTITY);
        eprintln!(
            "[slopos-compositor] {}",
            session_mode_note(backend_kind, output_scale)
        );
        // Prefer SLOPOS_OUTPUTS_LAYOUT (shell display arrange), else
        // SLOPOS_OUTPUTS + layout mode, else WIDTH/HEIGHT defaults.
        let resolved = resolve_laid_out_outputs_from_env();
        eprintln!("[slopos-compositor] {}", resolved.summary());
        let laid_out_outputs = resolved.laid_out.clone();
        let output_names = resolved.names.clone();
        let (outputs, output_globals, output_size) = create_outputs(
            &display_handle,
            &laid_out_outputs,
            &output_names,
            refresh_mhz,
            output_scale,
        );
        if resolved.laid_out.len() > 1 || !output_scale.is_identity() {
            eprintln!(
                "[slopos-compositor] multi-output/scale: {} heads, canvas {}x{} {}",
                resolved.laid_out.len(),
                output_size.w,
                output_size.h,
                output_scale_summary(output_scale)
            );
        }

        // -----------------------------------------------------------------------
        // Backend + GL renderer setup
        // -----------------------------------------------------------------------

        let x11_backend = if headless {
            None
        } else {
            Some(X11Backend::new().map_err(|err| {
                anyhow::anyhow!(
                    "requested nested backend could not initialize Smithay X11 transport: {err:#}"
                )
            })?)
        };

        let mut renderer_opt = None;
        let mut x11_surface_opt = None;

        if let Some(ref x11_backend) = x11_backend {
            let x11_handle = x11_backend.handle();
            let nested_window_size = Size::<u16, Logical>::from((
                output_size.w.clamp(1, u16::MAX as i32) as u16,
                output_size.h.clamp(1, u16::MAX as i32) as u16,
            ));
            if let Ok(window) = WindowBuilder::new()
                .title("slopos-compositor")
                .size(nested_window_size)
                .build(&x11_handle)
            {
                if let Ok((_drm_node, fd)) = x11_handle.drm_node() {
                    if let Ok(device) = GbmDevice::new(DeviceFd::from(fd)) {
                        if let Ok(egl_display) = unsafe { EGLDisplay::new(device.clone()) } {
                            if let Ok(egl_context) = EGLContext::new(&egl_display) {
                                let modifiers: HashSet<_> = egl_context
                                    .dmabuf_render_formats()
                                    .iter()
                                    .map(|fmt| fmt.modifier)
                                    .collect();
                                if let Ok(surf) = x11_handle.create_surface(
                                    &window,
                                    DmabufAllocator(GbmAllocator::new(
                                        device,
                                        GbmBufferFlags::RENDERING,
                                    )),
                                    modifiers.into_iter(),
                                ) {
                                    x11_surface_opt = Some(surf);
                                }
                                if let Ok(r) = unsafe { GlesRenderer::new(egl_context) } {
                                    renderer_opt = Some(r);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Wayland listening socket. Created only AFTER the X11 backend and GL
        // renderer are up: the socket name (and the wayland-display handshake
        // file the session entrypoint polls) must never be advertised by a
        // compositor that can still fail backend init and exit.
        let socket = ListeningSocketSource::new_auto()?;
        let socket_name = socket.socket_name().to_string_lossy().into_owned();
        tracing::info!("Listening on WAYLAND_DISPLAY={}", socket_name);
        eprintln!("[slopos-compositor] WAYLAND_DISPLAY={}", socket_name);
        println!("WAYLAND_DISPLAY={}", socket_name);
        // Bind the session control endpoint before publishing readiness. The
        // session supervisor starts shell clients as soon as readiness is
        // visible; constructing this listener later in `SloposCompositor`
        // otherwise leaves a startup window where menu actions are lost.
        let control_listener = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR")
            .map(PathBuf::from)
            .map(|runtime| bind_session_control_listener(&runtime))
            .transpose()?;
        // Write the actual socket name to a file so the entrypoint can read it,
        // and set the env var so child processes launched by the compositor see the right name.
        slopos_compositor::publish_session_readiness(&socket_name, output_size.w, output_size.h)
            .map_err(|err| anyhow::anyhow!("publish private session readiness: {err}"))?;
        std::env::set_var("SLOPOS_CLIENT_WAYLAND_DISPLAY", &socket_name);
        std::env::set_var("WAYLAND_DISPLAY", &socket_name);

        // Insert socket source: accept new Wayland client connections
        loop_handle
            .insert_source(socket, |client_stream, _, state| {
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .expect("failed to insert client");
            })
            .expect("failed to insert wayland socket source");
        register_wayland_display_source(&loop_handle, display)
            .context("insert Wayland display source")?;

        if let Some(x11_backend) = x11_backend {
            loop_handle
                .insert_source(x11_backend, |event, _, state| match event {
                    X11Event::CloseRequested { .. } => {
                        tracing::info!("X11 close requested");
                        state.running = false;
                    }
                    X11Event::Refresh { .. } | X11Event::PresentCompleted { .. } => {
                        // Coalesce host refresh with pending compositor damage.
                        state.request_redraw();
                    }
                    X11Event::Resized { new_size, .. } => {
                        state.handle_nested_x11_resize(new_size);
                    }
                    X11Event::Input { event, .. } => match event {
                        BackendInputEvent::Keyboard { event: ev } => {
                            handle_keyboard_event(state, &ev);
                        }
                        BackendInputEvent::PointerMotionAbsolute { event: ev } => {
                            handle_pointer_motion(state, &ev);
                        }
                        BackendInputEvent::PointerButton { event: ev } => {
                            handle_pointer_button(state, &ev);
                        }
                        BackendInputEvent::PointerAxis { event: ev } => {
                            handle_pointer_axis(state, &ev);
                        }
                        _ => {}
                    },
                    X11Event::Focus { .. } => {}
                })
                .expect("failed to insert x11 backend source");
        }

        // The session control socket is part of the nested event loop, not a
        // polled side-channel. This keeps the compositor asleep when idle
        // while still waking immediately for shell requests such as Minimize
        // or Fill. The listener is the exact socket bound in this session's
        // runtime directory; no Wayland socket discovery is involved.
        if let Some(listener) = control_listener {
            loop_handle
                .insert_source(
                    Generic::new(listener, Interest::READ, CalloopMode::Level),
                    |_, listener, state| {
                        for request in listener.drain() {
                            state.apply_session_control_request(request);
                        }
                        Ok(PostAction::Continue)
                    },
                )
                .map_err(|error| anyhow::anyhow!("insert session control socket: {error}"))?;
        }

        let initial_spaces = load_initial_spaces_model();
        let clock = Clock::<Monotonic>::new();
        let mut state = SloposCompositor {
            display_handle,
            _loop_signal: loop_signal,
            loop_handle,
            clock,
            compositor_state,
            shm_state,
            seat_state,
            _relative_pointer_state: relative_pointer_state,
            _pointer_constraints_state: pointer_constraints_state,
            xdg_shell_state,
            data_device_state,
            primary_selection_state,
            _output_manager_state: output_manager_state,
            xwayland_shell_state,
            layer_shell_state,
            foreign_toplevel_list,
            _xdg_decoration_state: xdg_decoration_state,
            _text_input_state: text_input_state,
            _input_method_state: input_method_state,
            im_popups: Vec::new(),
            seat,
            outputs,
            output_globals,
            disabled_output_globals: Vec::new(),
            laid_out_outputs,
            output_names,
            output_scale,
            refresh_mhz,
            backend_kind,
            outputs_revision: 0,
            running: true,
            windows: Vec::new(),
            workspace_state: WorkspaceState::new(),
            spaces: initial_spaces,
            spaces_session_epoch: new_session_epoch(),
            spaces_revision: 0,
            layer_surfaces: Vec::new(),
            popup_manager: PopupManager::default(),
            popup_grab: None,
            activated_window_id: None,
            last_minimized_window_id: None,
            next_window_offset: 0,
            pointer_pos: Point::from((0.0_f64, 0.0_f64)),
            last_backend_pointer_pos: None,
            cursor_status: CursorImageStatus::default_named(),
            headless_test_input_enabled: headless
                && std::env::var("SLOPOS_TEST_INPUT").ok().as_deref() == Some("1"),
            interactive_grab: None,
            x11_interactive_grab: None,
            workspace_swipe: WorkspaceSwipeRecognizer::default(),
            thumbnail_refresh_requested: false,
            left_button_down: false,
            last_pointer_press: None,
            frame_dirty: true,
            output_size,
            serial: 0,
            viewport_frame_revision: 0,
            renderer: renderer_opt,
            x11_surface: x11_surface_opt,
            clipboard_source: None,
            primary_source: None,
            clipboard_data: HashMap::new(),
            primary_data: HashMap::new(),
            server_dnd_data: HashMap::new(),
            dnd_icon: None,
            display_policy,
            hdr_caps,
            frame_scheduler,
            vrr_supported,
            display_policy_revision: 0,
            display_policy_fallback_reason,
            pending_damage: None,
            need_full_redraw: true, // first frame is always full
            placeholder_stats: PlaceholderPresentStats::new(),
            xwm: None,
            xdisplay: None,
            xwayland_client_id: None,
            xwayland_startup_watchdog_started: false,
            xwayland_recovery_budget: XWaylandRecoveryBudget::new(XWAYLAND_RESTART_BUDGET),
            x11_scene: X11SceneRegistry::default(),
            xwayland_keyboard_focus: None,
            wayland_socket_name: socket_name.clone(),
        };

        state.sync_legacy_workspace_state();
        state.reconcile_space_output_assignments();
        state.publish_spaces_state(false);
        state.publish_outputs_state();
        state.publish_display_policy_state();

        // P1.3: best-effort XWayland after state exists (needs loop_handle).
        try_start_xwayland(&mut state);

        tracing::info!("slopos-compositor event loop starting");
        while state.running {
            // Pace the loop with FrameScheduler when not adaptive (VRR).
            // Adaptive uses a short poll so PresentCompleted / input wake us quickly.
            let dispatch_timeout = if !state.frame_dirty {
                // File-descriptor sources, including the Wayland display, wake
                // calloop immediately. An idle compositor can therefore block
                // until input, a client request, or host output activity.
                None
            } else if state.frame_scheduler.refresh_rate().is_fixed() {
                let wait = state.frame_scheduler.time_until_next_frame();
                let ms = wait.as_millis().clamp(1, 32) as u64;
                Some(Duration::from_millis(ms))
            } else {
                Some(Duration::from_millis(16))
            };

            event_loop.dispatch(dispatch_timeout, &mut state)?;

            // Damage-driven rendering: commits, pointer motion, output refresh,
            // workspace changes and animations explicitly mark the frame dirty.
            // Static desktops therefore sleep instead of saturating LLVMpipe.
            if state.frame_dirty {
                state.render_frame();
            }
        }

        tracing::info!("slopos-compositor exiting");
        Ok(())
    }

    #[cfg(test)]
    mod nested_resize_tests {
        use super::*;

        #[test]
        fn uses_the_swapchain_physical_extent() {
            assert_eq!(
                SloposCompositor::nested_x11_resize_output_size(Size::from((1193_u16, 768_u16))),
                Size::<i32, Physical>::from((1193, 768)),
            );
            assert_eq!(
                SloposCompositor::nested_x11_resize_output_size(Size::from((0_u16, 0_u16))),
                Size::<i32, Physical>::from((1, 1)),
            );
            assert_eq!(
                SloposCompositor::nested_x11_resize_logical_output_size(
                    Size::from((2560_u16, 1600_u16)),
                    OutputScale::new(2, 1).unwrap(),
                ),
                Size::<i32, Logical>::from((1280, 800)),
            );
            assert_eq!(
                SloposCompositor::nested_x11_resize_logical_output_size(
                    Size::from((1_u16, 1_u16)),
                    OutputScale::new(3, 2).unwrap(),
                ),
                Size::<i32, Logical>::from((1, 1)),
            );
        }

        #[test]
        fn nested_render_geometry_matrix_keeps_requested_and_effective_scale_consistent() {
            let logical = (1024, 768);
            let point = Point::<i32, Logical>::from((101, 77));
            let cases = [
                (OutputScale::new(1, 1).unwrap(), (1024, 768), (101, 77)),
                (OutputScale::new(5, 4).unwrap(), (1280, 960), (126, 96)),
                (OutputScale::new(3, 2).unwrap(), (1536, 1152), (152, 116)),
                (OutputScale::new(2, 1).unwrap(), (2048, 1536), (202, 154)),
            ];

            for (requested, expected_size, expected_point) in cases {
                let effective = SloposCompositor::effective_nested_output_scale(requested);
                assert_eq!(effective, requested);
                assert_eq!(scale_logical_to_physical(logical, effective), expected_size);
                assert_eq!(
                    SloposCompositor::nested_logical_point_to_physical(point, effective),
                    Point::<i32, Physical>::from(expected_point),
                );
            }

            let unreduced = OutputScale::new(10, 8).unwrap();
            assert_eq!(
                SloposCompositor::effective_nested_output_scale(unreduced),
                OutputScale::new(5, 4).unwrap(),
            );
        }
    }

    #[cfg(test)]
    mod axis_frame_tests {
        use super::*;

        #[test]
        fn axis_frame_keeps_timestamp_directions_and_both_value_forms() {
            let frame = build_axis_frame(AxisFrameInput {
                time: 1234,
                source: AxisSource::Continuous,
                directions: (
                    AxisRelativeDirection::Inverted,
                    AxisRelativeDirection::Identical,
                ),
                amounts: (Some(1.5), Some(-2.25)),
                v120: (Some(60.0), Some(-120.0)),
            });

            assert_eq!(frame.time, 1234);
            assert_eq!(frame.source, Some(AxisSource::Continuous));
            assert_eq!(
                frame.relative_direction,
                (
                    AxisRelativeDirection::Inverted,
                    AxisRelativeDirection::Identical,
                )
            );
            assert_eq!(frame.axis, (1.5, -2.25));
            assert_eq!(frame.v120, Some((60, -120)));
        }
    }

    #[cfg(test)]
    mod x11_scene_tests {
        use super::*;

        #[test]
        fn scene_entry_becomes_visible_only_after_map_and_association() {
            let geometry = Rectangle::new(Point::from((24, 36)), Size::from((320, 200)));
            let mut state = X11SceneEntryState::new(geometry);
            assert!(!state.visible());
            state.set_associated(true);
            assert!(!state.visible());
            state.set_mapped(true);
            assert!(state.visible());
            state.set_mapped(false);
            assert!(!state.visible());
        }

        #[test]
        fn scene_origin_and_hit_test_use_compositor_coordinates() {
            let geometry = Rectangle::new(Point::from((120, 80)), Size::from((200, 100)));
            assert_eq!(
                SloposCompositor::x11_surface_scene_origin(geometry),
                Point::from((120, 80))
            );
            assert_eq!(
                SloposCompositor::x11_surface_scene_hit(geometry, Point::from((140.5, 95.25))),
                Some(Point::from((20.5, 15.25)))
            );
            assert_eq!(
                SloposCompositor::x11_surface_scene_hit(geometry, Point::from((320.0, 95.0))),
                None
            );
        }

        #[test]
        fn x11_space_visibility_uses_authoritative_membership_and_survives_move() {
            let mut spaces = SpacesModel::with_default_count(2).expect("default spaces");
            let first = spaces.active_space();
            let second = spaces.space_ids()[1];
            let window = X11Window::from(0x400020_u32);
            let key = x11_space_window_id(window);

            spaces
                .assign_window_to_current(key.clone())
                .expect("assign X11 window");
            assert!(x11_window_visible_on_space(&spaces, window, first));
            assert!(!x11_window_visible_on_space(&spaces, window, second));

            spaces
                .move_window(key, SpaceTarget::Id(second))
                .expect("move X11 window");
            assert!(!x11_window_visible_on_space(&spaces, window, first));
            assert!(x11_window_visible_on_space(&spaces, window, second));
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use linux::parse_bool_env;
    use smithay::utils::Point;

    #[test]
    fn test_parse_bool_env() {
        std::env::set_var("TEST_BOOL_ENV_TRUE_1", "1");
        std::env::set_var("TEST_BOOL_ENV_TRUE_2", "true");
        std::env::set_var("TEST_BOOL_ENV_TRUE_3", "YES");
        std::env::set_var("TEST_BOOL_ENV_TRUE_4", "On");
        std::env::set_var("TEST_BOOL_ENV_FALSE_1", "0");
        std::env::set_var("TEST_BOOL_ENV_FALSE_2", "false");
        std::env::set_var("TEST_BOOL_ENV_FALSE_3", "no");
        std::env::set_var("TEST_BOOL_ENV_FALSE_4", "OFF");

        assert!(parse_bool_env("TEST_BOOL_ENV_TRUE_1"));
        assert!(parse_bool_env("TEST_BOOL_ENV_TRUE_2"));
        assert!(parse_bool_env("TEST_BOOL_ENV_TRUE_3"));
        assert!(parse_bool_env("TEST_BOOL_ENV_TRUE_4"));
        assert!(!parse_bool_env("TEST_BOOL_ENV_FALSE_1"));
        assert!(!parse_bool_env("TEST_BOOL_ENV_FALSE_2"));
        assert!(!parse_bool_env("TEST_BOOL_ENV_FALSE_3"));
        assert!(!parse_bool_env("TEST_BOOL_ENV_FALSE_4"));
        assert!(!parse_bool_env("TEST_BOOL_ENV_UNSET"));
    }

    #[test]
    fn automatic_backend_requires_x11_display_for_nested() {
        assert_eq!(linux::default_backend_for_host(Some(":99"), None), "nested");
        assert_eq!(
            linux::default_backend_for_host(None, Some("wayland-0")),
            "drm",
            "a Wayland-only host must not select the X11 nested backend"
        );
        assert_eq!(linux::default_backend_for_host(Some(""), None), "drm");
        assert_eq!(linux::default_backend_for_host(None, None), "drm");
    }

    #[test]
    fn explicit_nested_backend_fails_without_x11_display() {
        let error = linux::validate_nested_transport("nested", None).unwrap_err();
        assert!(error.contains("DISPLAY"));
        assert!(linux::validate_nested_transport("nested", Some(":99")).is_ok());
        assert!(linux::validate_nested_transport("drm", None).is_ok());
        assert!(linux::validate_nested_transport("headless", None).is_ok());
    }

    #[test]
    fn nested_layer_surface_hit_origin_is_translated_to_compositor_space() {
        assert_eq!(
            linux::layer_surface_hit_origin(Point::from((120, 80)), Point::from((7, 11))),
            Point::from((127.0, 91.0)),
        );
    }

    #[test]
    fn nested_control_binds_before_readiness_and_delivers_request() {
        let runtime = std::env::temp_dir().join(format!(
            "slopos-nested-control-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before Unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&runtime).expect("create test runtime");

        let listener = linux::bind_session_control_listener(&runtime)
            .expect("bind control listener before readiness");
        let control_socket = runtime.join(slopos_bus::SESSION_CONTROL_SOCKET);
        assert!(
            control_socket.exists(),
            "control socket must precede readiness"
        );

        std::fs::write(runtime.join("readiness"), b"wayland-9\n").expect("write readiness marker");
        let request = slopos_bus::SessionControlRequest::FocusedWindow {
            action: slopos_bus::WindowPresentationAction::Fill,
        };
        let previous_runtime = std::env::var_os("SLOPOS_SESSION_RUNTIME_DIR");
        std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", &runtime);
        slopos_bus::send_session_control(&request).expect("deliver semantic request");
        if let Some(previous_runtime) = previous_runtime {
            std::env::set_var("SLOPOS_SESSION_RUNTIME_DIR", previous_runtime);
        } else {
            std::env::remove_var("SLOPOS_SESSION_RUNTIME_DIR");
        }

        assert_eq!(listener.drain(), vec![request]);
        drop(listener);
        std::fs::remove_dir_all(&runtime).expect("remove test runtime");
    }

    #[test]
    fn nested_control_source_wakes_calloop_and_drains_request() {
        use std::os::unix::net::UnixDatagram;
        use std::time::{Duration, Instant};

        use smithay::reexports::calloop::{
            generic::Generic, EventLoop, Interest, Mode as CalloopMode, PostAction,
        };

        let runtime = std::env::temp_dir().join(format!(
            "slo-evt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before Unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&runtime).expect("create test runtime");

        let listener = linux::bind_session_control_listener(&runtime)
            .expect("bind exact session control socket");
        let sender = UnixDatagram::unbound().expect("create control sender");
        let request = slopos_bus::SessionControlRequest::FocusedWindow {
            action: slopos_bus::WindowPresentationAction::Fill,
        };

        let mut event_loop: EventLoop<Vec<slopos_bus::SessionControlRequest>> =
            EventLoop::try_new().expect("create calloop");
        event_loop
            .handle()
            .insert_source(
                Generic::new(listener, Interest::READ, CalloopMode::Level),
                |_, listener, requests| {
                    requests.extend(listener.drain());
                    Ok(PostAction::Continue)
                },
            )
            .expect("register exact control fd");

        // Do not queue the datagram before dispatch: the sender waits briefly
        // after dispatch is entered so the test exercises an idle poll wake.
        let send_after = Duration::from_millis(50);
        let payload = serde_json::to_vec(&request).expect("serialize control request");
        let socket_path = runtime.join(slopos_bus::SESSION_CONTROL_SOCKET);
        let sender_thread = std::thread::spawn(move || {
            std::thread::sleep(send_after);
            sender
                .send_to(&payload, socket_path)
                .expect("send control request");
        });

        let dispatch_timeout = Duration::from_secs(1);
        let dispatch_started = Instant::now();
        let mut observed = Vec::new();
        event_loop
            .dispatch(Some(dispatch_timeout), &mut observed)
            .expect("dispatch control fd");
        let dispatch_elapsed = dispatch_started.elapsed();
        sender_thread.join().expect("join control sender");

        assert!(
            dispatch_elapsed >= Duration::from_millis(25),
            "dispatch returned before the delayed request could wake it: {dispatch_elapsed:?}"
        );
        assert!(
            dispatch_elapsed < dispatch_timeout,
            "dispatch reached its timeout instead of waking for the request: {dispatch_elapsed:?}"
        );
        assert_eq!(observed, vec![request]);

        drop(event_loop);
        std::fs::remove_dir_all(&runtime).expect("remove test runtime");
    }

    #[test]
    fn x11_resize_edges_use_shared_interactive_mapping() {
        assert_eq!(
            linux::x11_resize_edge_to_resize_edges(smithay::xwayland::xwm::ResizeEdge::Top),
            slopos_compositor::ResizeEdges::TOP
        );
        assert_eq!(
            linux::x11_resize_edge_to_resize_edges(smithay::xwayland::xwm::ResizeEdge::BottomRight,),
            slopos_compositor::ResizeEdges::BOTTOM_RIGHT
        );
    }

    #[test]
    fn xwayland_recovery_budget_is_session_scoped_and_bounded() {
        let mut budget = linux::XWaylandRecoveryBudget::new(3);

        assert_eq!(budget.remaining(), 3);
        assert!(budget.take_restart());
        assert!(budget.take_restart());
        assert!(budget.take_restart());
        assert_eq!(budget.remaining(), 0);
        assert!(!budget.take_restart());
        assert!(!budget.take_restart());
        assert_eq!(budget.remaining(), 0);
    }
}
