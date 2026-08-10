//! Best-effort AT-SPI accessibility event emission for the shell.
//!
//! # Honest contract
//! - **In-process always works**: every event is pushed onto a process-local
//!   [`AccessibilityEventBus`] for toolkit consumers and tests.
//! - **D-Bus is best-effort**: when `slopos_kit` holds an AT-SPI registration
//!   connection (see `register_at_spi_shell_chrome` in `main`), events are also
//!   signalled via zbus (`org.a11y.atspi.Event.Focus` / `Event.Object`). If no
//!   connection exists (macOS CI, no session bus, register skipped), D-Bus
//!   emission is skipped without error — only the in-process bus retains the event.
//! - **Registry may still not deliver to Orca** even when emit returns success
//!   (Embed failed, no listener). Do not claim Orca-complete event delivery.
//!
//! Pure path helpers (`chrome_focus_atspi_path`, serialize via kit) are host-safe.

use std::sync::{Mutex, OnceLock};

use slopos_kit::{
    at_spi_connection_available, atspi_object_path, serialize_event_for_dbus,
    try_emit_atspi_dbus_event, AccessibilityEventBus, AccessibleEvent, AccessibleEventKind,
    SerializedAtspiEvent,
};

use crate::chrome_protocol::ChromeFocusTarget;

/// Process-local accessibility event bus (always available).
fn in_process_bus() -> &'static Mutex<AccessibilityEventBus> {
    static BUS: OnceLock<Mutex<AccessibilityEventBus>> = OnceLock::new();
    BUS.get_or_init(|| Mutex::new(AccessibilityEventBus::new()))
}

/// Result of a best-effort accessibility event publish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitAccessibleResult {
    /// Event was queued on the in-process bus (always true on success path).
    pub in_process: bool,
    /// D-Bus signal was successfully sent on the registration connection.
    pub dbus_emitted: bool,
}

/// Snapshot of whether a kit AT-SPI registration connection is currently held.
pub fn atspi_dbus_connection_available() -> bool {
    at_spi_connection_available()
}

/// Map shell chrome focus targets to structural shell AT-SPI object paths.
///
/// Indices match [`slopos_kit::shell_chrome_accessibility_tree`]:
/// menu bar `0`, desktop `1`, dock `2`, window frame `3`.
pub fn chrome_focus_atspi_path(target: ChromeFocusTarget) -> String {
    let index = match target {
        ChromeFocusTarget::MenuBar => 0,
        ChromeFocusTarget::DesktopIcons => 1,
        ChromeFocusTarget::Dock => 2,
        ChromeFocusTarget::Windows => 3,
    };
    atspi_object_path(index)
}

/// Pure serialize of a chrome Focus event (for tests / callers that need fields only).
pub fn serialize_chrome_focus_for_dbus(target: ChromeFocusTarget) -> SerializedAtspiEvent {
    let event = AccessibleEvent::focus(chrome_focus_atspi_path(target));
    serialize_event_for_dbus(&event)
}

/// Push `event` to the in-process bus and best-effort emit on the AT-SPI D-Bus
/// connection when present (reuses kit registration connection).
pub fn try_emit_accessible_event(event: AccessibleEvent) -> EmitAccessibleResult {
    // 1. In-process — always.
    let in_process = if let Ok(mut bus) = in_process_bus().lock() {
        bus.push(event.clone());
        true
    } else {
        tracing::warn!("accessibility in-process bus mutex poisoned; skipping local queue");
        false
    };

    // 2. D-Bus — best-effort via kit registration connection.
    let dbus_emitted = try_emit_atspi_dbus_event(&event);
    if dbus_emitted {
        tracing::debug!(
            path = %event.path,
            kind = event.kind.as_str(),
            "AT-SPI event emitted on D-Bus (best-effort)"
        );
    } else {
        tracing::trace!(
            path = %event.path,
            kind = event.kind.as_str(),
            connection = at_spi_connection_available(),
            "AT-SPI D-Bus emit skipped or failed; in-process bus retained event"
        );
    }

    EmitAccessibleResult {
        in_process,
        dbus_emitted,
    }
}

/// Emit a Focus event for the shell chrome region after Tab / Shift+Tab navigation.
///
/// Always records in-process; D-Bus only when registration connection exists.
pub fn emit_chrome_focus(target: ChromeFocusTarget) -> EmitAccessibleResult {
    let path = chrome_focus_atspi_path(target);
    let result = try_emit_accessible_event(AccessibleEvent::focus(path));
    // Companion StateChanged(focused=1) is useful for ATs that listen on Object.
    let _ = try_emit_accessible_event(AccessibleEvent::state_changed(
        chrome_focus_atspi_path(target),
        "focused",
        true,
    ));
    result
}

/// Drain pending in-process events (FIFO). Used by tests and optional shell consumers.
pub fn drain_in_process_events() -> Vec<AccessibleEvent> {
    in_process_bus()
        .lock()
        .map(|mut bus| bus.drain())
        .unwrap_or_default()
}

/// Number of pending in-process events.
pub fn in_process_event_count() -> usize {
    in_process_bus().lock().map(|bus| bus.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_kit::{ATSPI_EVENT_FOCUS_IFACE, ATSPI_EVENT_OBJECT_IFACE};

    #[test]
    fn chrome_focus_paths_match_shell_tree_indices() {
        assert_eq!(
            chrome_focus_atspi_path(ChromeFocusTarget::MenuBar),
            "/org/a11y/atspi/accessible/0"
        );
        assert_eq!(
            chrome_focus_atspi_path(ChromeFocusTarget::DesktopIcons),
            "/org/a11y/atspi/accessible/1"
        );
        assert_eq!(
            chrome_focus_atspi_path(ChromeFocusTarget::Dock),
            "/org/a11y/atspi/accessible/2"
        );
        assert_eq!(
            chrome_focus_atspi_path(ChromeFocusTarget::Windows),
            "/org/a11y/atspi/accessible/3"
        );
    }

    #[test]
    fn serialize_chrome_focus_is_focus_interface() {
        let s = serialize_chrome_focus_for_dbus(ChromeFocusTarget::MenuBar);
        assert_eq!(s.interface, ATSPI_EVENT_FOCUS_IFACE);
        assert_eq!(s.member, "Focus");
        assert_eq!(s.path, "/org/a11y/atspi/accessible/0");
        assert_eq!(s.detail1, 1);
    }

    /// Serialize tests that share the process-global in-process bus.
    fn bus_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn try_emit_always_queues_in_process() {
        let _guard = bus_test_lock();
        // Drain any leftover state from other tests in this binary.
        let _ = drain_in_process_events();

        let path = chrome_focus_atspi_path(ChromeFocusTarget::Dock);
        let result = try_emit_accessible_event(AccessibleEvent::focus(&path));
        assert!(result.in_process);
        // D-Bus is environment-dependent; never required for this test.
        let _ = result.dbus_emitted;

        let events = drain_in_process_events();
        assert!(
            events
                .iter()
                .any(|e| e.kind == AccessibleEventKind::Focus && e.path == path),
            "expected Focus for {path}, got {events:?}"
        );
    }

    #[test]
    fn emit_chrome_focus_queues_focus_and_state() {
        let _guard = bus_test_lock();
        let _ = drain_in_process_events();
        let r = emit_chrome_focus(ChromeFocusTarget::DesktopIcons);
        assert!(r.in_process);

        let events = drain_in_process_events();
        let path = chrome_focus_atspi_path(ChromeFocusTarget::DesktopIcons);
        assert!(events
            .iter()
            .any(|e| e.kind == AccessibleEventKind::Focus && e.path == path));
        assert!(events.iter().any(|e| {
            e.kind == AccessibleEventKind::StateChanged
                && e.path == path
                && e.any_data == "focused"
                && e.detail1 == 1
        }));

        // Pure serialization of companion StateChanged for the accessibility contract.
        let state = AccessibleEvent::state_changed(&path, "focused", true);
        let s = serialize_event_for_dbus(&state);
        assert_eq!(s.interface, ATSPI_EVENT_OBJECT_IFACE);
        assert_eq!(s.member, "StateChanged");
        assert_eq!(s.detail, "focused");
    }
}
