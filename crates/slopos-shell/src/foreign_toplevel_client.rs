//! Live `ext-foreign-toplevel-list-v1` client → [`ForeignToplevelRegistry`].
//!
//! Pure event application is unit-tested. On Linux, [`try_sync_foreign_toplevels`]
//! connects to `WAYLAND_DISPLAY`, binds the global, and fills the registry from
//! compositor-owned windows (not shell paint-rects alone).

use crate::foreign_toplevel::{ForeignToplevelEntry, ForeignToplevelRegistry};

/// Protocol event applied to the registry (pure; mirrors compositor list events).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForeignToplevelListEvent {
    /// New or updated handle.
    Toplevel {
        handle_id: String,
        title: String,
        app_id: String,
    },
    /// Handle closed.
    Closed { handle_id: String },
    /// List finished initial dump.
    Finished,
}

/// Apply one protocol event to the registry (shipped pure path).
pub fn apply_foreign_toplevel_list_event(
    registry: &mut ForeignToplevelRegistry,
    event: ForeignToplevelListEvent,
) {
    match event {
        ForeignToplevelListEvent::Toplevel {
            handle_id,
            title,
            app_id,
        } => {
            if registry.get(&handle_id).is_some() {
                registry.update(&handle_id, Some(title), Some(app_id), None);
            } else {
                registry.add(ForeignToplevelEntry::new(handle_id, title, app_id, None));
            }
        }
        ForeignToplevelListEvent::Closed { handle_id } => {
            registry.close(&handle_id);
        }
        ForeignToplevelListEvent::Finished => {}
    }
}

/// Apply a batch of events (as received during a roundtrip).
pub fn apply_foreign_toplevel_list_events(
    registry: &mut ForeignToplevelRegistry,
    events: &[ForeignToplevelListEvent],
) {
    for e in events {
        apply_foreign_toplevel_list_event(registry, e.clone());
    }
}

/// Best-effort: bind `ext_foreign_toplevel_list_v1` and sync into `registry`.
///
/// Returns number of toplevels after sync, or `None` if unavailable.
pub fn try_sync_foreign_toplevels(registry: &mut ForeignToplevelRegistry) -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        match linux::sync_once(registry) {
            Ok(n) => {
                tracing::info!(
                    toplevels = n,
                    "foreign-toplevel-list synced from compositor"
                );
                Some(n)
            }
            Err(err) => {
                tracing::debug!(error = %err, "foreign-toplevel-list sync skipped");
                None
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = registry;
        None
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use wayland_client::{protocol::wl_registry, Connection, Dispatch, Proxy, QueueHandle};
    use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
        ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
    };

    struct State {
        list: Option<ExtForeignToplevelListV1>,
        events: Vec<ForeignToplevelListEvent>,
        /// handle ptr id → pending title/app
        pending: std::collections::HashMap<u32, (String, String, String)>,
    }

    pub fn sync_once(registry: &mut ForeignToplevelRegistry) -> Result<usize, String> {
        let _ =
            std::env::var("WAYLAND_DISPLAY").map_err(|_| "WAYLAND_DISPLAY unset".to_string())?;
        let conn = Connection::connect_to_env().map_err(|e| format!("wayland connect: {e}"))?;
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();
        let display = conn.display();
        let _reg = display.get_registry(&qh, ());

        let mut state = State {
            list: None,
            events: Vec::new(),
            pending: std::collections::HashMap::new(),
        };
        event_queue
            .roundtrip(&mut state)
            .map_err(|e| format!("registry roundtrip: {e}"))?;

        if state.list.is_none() {
            return Err("ext_foreign_toplevel_list_v1 missing".into());
        }

        // Drain initial list events
        for _ in 0..16 {
            let _ = event_queue.dispatch_pending(&mut state);
            let _ = conn.flush();
            let _ = event_queue.roundtrip(&mut state);
            if state
                .events
                .iter()
                .any(|e| matches!(e, ForeignToplevelListEvent::Finished))
            {
                break;
            }
        }

        apply_foreign_toplevel_list_events(registry, &state.events);
        Ok(registry.len())
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for State {
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
                if interface == "ext_foreign_toplevel_list_v1" {
                    let v = version.min(1);
                    state.list =
                        Some(registry.bind::<ExtForeignToplevelListV1, _, _>(name, v, qh, ()));
                }
            }
        }
    }

    impl Dispatch<ExtForeignToplevelListV1, ()> for State {
        fn event(
            state: &mut Self,
            _: &ExtForeignToplevelListV1,
            event: ext_foreign_toplevel_list_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } => {
                    let id = toplevel.id().protocol_id();
                    state
                        .pending
                        .insert(id, (format!("ftl-{id}"), String::new(), String::new()));
                }
                ext_foreign_toplevel_list_v1::Event::Finished => {
                    state.events.push(ForeignToplevelListEvent::Finished);
                }
                _ => {}
            }
        }

        // The `toplevel` event carries a new_id: wayland-client requires this
        // specialization to build the child proxy's dispatch data, and its
        // default implementation is a hard `panic!` in a non-unwinding
        // context — i.e. without this, the first toplevel a real compositor
        // (labwc, KDE) announces aborts the whole shell. Found live on
        // 2026-07-27: opening Force Quit under labwc killed the session.
        wayland_client::event_created_child!(State, ExtForeignToplevelListV1, [
            ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
        ]);
    }

    impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
        fn event(
            state: &mut Self,
            handle: &ExtForeignToplevelHandleV1,
            event: ext_foreign_toplevel_handle_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let id = handle.id().protocol_id();
            match event {
                ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                    if let Some(e) = state.pending.get_mut(&id) {
                        e.1 = title;
                    }
                }
                ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                    if let Some(e) = state.pending.get_mut(&id) {
                        e.2 = app_id;
                    }
                }
                ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                    if let Some(e) = state.pending.get_mut(&id) {
                        e.0 = identifier;
                    }
                }
                ext_foreign_toplevel_handle_v1::Event::Done => {
                    if let Some((hid, title, app_id)) = state.pending.get(&id).cloned() {
                        state.events.push(ForeignToplevelListEvent::Toplevel {
                            handle_id: hid,
                            title,
                            app_id,
                        });
                    }
                }
                ext_foreign_toplevel_handle_v1::Event::Closed => {
                    let hid = state
                        .pending
                        .get(&id)
                        .map(|e| e.0.clone())
                        .unwrap_or_else(|| format!("ftl-{id}"));
                    state
                        .events
                        .push(ForeignToplevelListEvent::Closed { handle_id: hid });
                    state.pending.remove(&id);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_toplevel_and_closed() {
        let mut reg = ForeignToplevelRegistry::new();
        apply_foreign_toplevel_list_event(
            &mut reg,
            ForeignToplevelListEvent::Toplevel {
                handle_id: "h1".into(),
                title: "Finder".into(),
                app_id: "finder".into(),
            },
        );
        assert_eq!(reg.len(), 1);
        assert!(reg.force_quit_labels().iter().any(|l| l.contains("Finder")));
        apply_foreign_toplevel_list_event(
            &mut reg,
            ForeignToplevelListEvent::Closed {
                handle_id: "h1".into(),
            },
        );
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn apply_update_existing() {
        let mut reg = ForeignToplevelRegistry::new();
        apply_foreign_toplevel_list_events(
            &mut reg,
            &[
                ForeignToplevelListEvent::Toplevel {
                    handle_id: "a".into(),
                    title: "Old".into(),
                    app_id: "app".into(),
                },
                ForeignToplevelListEvent::Toplevel {
                    handle_id: "a".into(),
                    title: "New".into(),
                    app_id: "app".into(),
                },
            ],
        );
        assert_eq!(reg.len(), 1);
        let labels = reg.force_quit_labels();
        assert!(labels.iter().any(|l| l.contains("New")));
        assert!(!labels.iter().any(|l| l.contains("Old")));
    }

    #[test]
    fn try_sync_without_wayland_is_safe() {
        let mut reg = ForeignToplevelRegistry::new();
        let _ = try_sync_foreign_toplevels(&mut reg);
    }
}
