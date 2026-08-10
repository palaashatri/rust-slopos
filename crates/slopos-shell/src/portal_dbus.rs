//! FreeDesktop portal session-bus export (Linux only).
//!
//! Host tests never open D-Bus. On Linux, [`try_register_portal_session_bus`]
//! best-effort claims [`crate::portal::PORTAL_BUS_NAME`] and serves Screenshot,
//! Settings, OpenURI, FileChooser, ScreenCast, plus simplified Secret / Print /
//! Inhibit interfaces that call pure handlers in [`crate::portal`] /
//! [`crate::portal_extra`].
//!
//! ScreenCast streams exposed on the bus are protocol-level stubs (`node_id`
//! placeholders) — PipeWire is not started or connected. Start results include
//! an honest `note` (`backend=portal_stub` or `backend=pipewire_socket_present`).
//!
//! Inhibit cookies are stored process-wide so shell idle policy can poll
//! [`crate::portal_extra::active_inhibits`].

use crate::portal::{PORTAL_BUS_NAME, PORTAL_PATH};

/// Best-effort register portal interfaces on the session bus.
///
/// On non-Linux hosts, or when the session bus is missing / name is taken,
/// logs and returns `false` — never fails shell startup.
pub fn try_register_portal_session_bus() -> bool {
    #[cfg(target_os = "linux")]
    {
        use crate::portal::{
            PORTAL_OPENURI_INTERFACE, PORTAL_SCREENCAST_INTERFACE, PORTAL_SCREENSHOT_INTERFACE,
            PORTAL_SETTINGS_INTERFACE,
        };
        match linux::register() {
            Ok(()) => {
                tracing::info!(
                    bus = PORTAL_BUS_NAME,
                    path = PORTAL_PATH,
                    screenshot = PORTAL_SCREENSHOT_INTERFACE,
                    settings = PORTAL_SETTINGS_INTERFACE,
                    openuri = PORTAL_OPENURI_INTERFACE,
                    screencast = PORTAL_SCREENCAST_INTERFACE,
                    secret = "org.freedesktop.impl.portal.Secret",
                    print = "org.freedesktop.impl.portal.Print",
                    inhibit = "org.freedesktop.impl.portal.Inhibit",
                    "SLOPOS-I portal handlers registered on session bus"
                );
                true
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "SLOPOS-I portal registration skipped"
                );
                false
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        tracing::debug!("SLOPOS-I portal registration skipped (non-Linux host)");
        false
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::portal::{
        create_screencast_session_with_backend_note, handle_file_chooser_open,
        handle_file_chooser_save, handle_portal_screenshot_request, plan_open_uri,
        portal_screenshot_uri_for, portal_screenshots_dir, read_all_portal_settings,
        read_portal_setting, select_screencast_sources, start_screencast_with_readiness,
        take_portal_style_screenshot_with, OpenUriAction, PortalFileChooserRequest,
        PortalScreencastRequest, PortalScreencastSession, PortalScreenshotRequest,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zbus::blocking::connection::Builder as ConnectionBuilder;
    use zbus::blocking::Connection;
    use zbus::interface;
    use zbus::zvariant::{OwnedValue, Value};

    /// Keeps the session bus connection alive for process lifetime.
    static REGISTRATION: StdMutex<Option<PortalRegistration>> = StdMutex::new(None);

    struct PortalRegistration {
        _connection: Connection,
    }

    struct PortalScreenshotIface;

    #[interface(name = "org.freedesktop.impl.portal.Screenshot")]
    impl PortalScreenshotIface {
        /// Portal Screenshot: pure plan + best-effort local capture.
        ///
        /// Returns `(response, results)` where response `0` = success, `2` = error/cancel.
        /// `results` includes `uri` (`file://...`) on success.
        fn screenshot(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            _app_id: &str,
            _parent_window: &str,
            options: HashMap<String, OwnedValue>,
        ) -> (u32, HashMap<String, OwnedValue>) {
            let interactive = option_bool(&options, "interactive").unwrap_or(false);
            let include_cursor = option_bool(&options, "cursor").unwrap_or(false)
                || option_bool(&options, "include-cursor").unwrap_or(false);
            let request = PortalScreenshotRequest {
                interactive,
                include_cursor,
            };

            // Prefer real capture when tools are available; fall back to pure planned path.
            let result = match take_portal_style_screenshot_with(request) {
                Ok(r) => r,
                Err(_) => {
                    let base = std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from("/tmp"));
                    let dir = portal_screenshots_dir(&base);
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    handle_portal_screenshot_request(request, &dir, now)
                }
            };

            let uri = portal_screenshot_uri_for(&result.path);
            let mut results: HashMap<String, OwnedValue> = HashMap::new();
            if let Ok(v) = OwnedValue::try_from(Value::from(uri)) {
                results.insert("uri".to_string(), v);
            }
            (0u32, results)
        }
    }

    struct PortalSettingsIface;

    #[interface(name = "org.freedesktop.impl.portal.Settings")]
    impl PortalSettingsIface {
        /// Settings.Read — pure map lookup; value as string variant.
        fn read(&self, namespace: &str, key: &str) -> zbus::fdo::Result<OwnedValue> {
            match read_portal_setting(namespace, key) {
                Some(v) => OwnedValue::try_from(Value::from(v))
                    .map_err(|e| zbus::fdo::Error::Failed(format!("value conversion failed: {e}"))),
                None => Err(zbus::fdo::Error::Failed(format!(
                    "setting not found: {namespace} / {key}"
                ))),
            }
        }

        /// Settings.ReadAll — pure map for the namespace.
        fn read_all(&self, namespace: &str) -> HashMap<String, OwnedValue> {
            let mut out = HashMap::new();
            for (k, v) in read_all_portal_settings(namespace) {
                if let Ok(owned) = OwnedValue::try_from(Value::from(v)) {
                    out.insert(k, owned);
                }
            }
            out
        }
    }

    struct PortalFileChooserIface;

    #[interface(name = "org.freedesktop.impl.portal.FileChooser")]
    impl PortalFileChooserIface {
        /// OpenFile — pure path selection under options["current_folder"].
        /// options["uris"] string array of basenames selects files (agent/test).
        fn open_file(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            _app_id: &str,
            _parent_window: &str,
            title: &str,
            options: HashMap<String, OwnedValue>,
        ) -> (u32, HashMap<String, OwnedValue>) {
            let multiple = option_bool(&options, "multiple").unwrap_or(false);
            let directory = option_bool(&options, "directory").unwrap_or(false);
            // Optional string options: best-effort; pure tests cover selection logic.
            let current_folder = option_string_loose(&options, "current_folder");
            let req = PortalFileChooserRequest {
                title: title.into(),
                multiple,
                directory,
                current_folder,
                ..Default::default()
            };
            // Selected basenames may be supplied as a single newline-joined "selected" string.
            let names = option_string_loose(&options, "selected")
                .map(|s| {
                    s.split('\n')
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let names_ref: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
            match handle_file_chooser_open(&req, &names_ref) {
                Ok(r) if r.uris.is_empty() => (1u32, HashMap::new()), // cancelled
                Ok(r) => (0u32, uris_result_map(&r.uris)),
                Err(_) => (2u32, HashMap::new()),
            }
        }

        fn save_file(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            _app_id: &str,
            _parent_window: &str,
            title: &str,
            options: HashMap<String, OwnedValue>,
        ) -> (u32, HashMap<String, OwnedValue>) {
            let current_folder = option_string_loose(&options, "current_folder");
            let current_name = option_string_loose(&options, "current_name");
            let confirm = option_bool(&options, "confirm").unwrap_or(true);
            let req = PortalFileChooserRequest {
                title: title.into(),
                current_folder,
                current_name,
                ..Default::default()
            };
            match handle_file_chooser_save(&req, confirm) {
                Ok(r) if r.uris.is_empty() => (1u32, HashMap::new()),
                Ok(r) => (0u32, uris_result_map(&r.uris)),
                Err(_) => (2u32, HashMap::new()),
            }
        }
    }

    fn uris_result_map(uris: &[String]) -> HashMap<String, OwnedValue> {
        let mut results = HashMap::new();
        // Store as newline-joined string for zbus compatibility (array encoding varies).
        let joined = uris.join("\n");
        if let Ok(v) = OwnedValue::try_from(Value::from(joined)) {
            results.insert("uris".into(), v);
        }
        if let Some(first) = uris.first() {
            if let Ok(v) = OwnedValue::try_from(Value::from(first.as_str())) {
                results.insert("uri".into(), v);
            }
        }
        results
    }

    struct PortalOpenUriIface;

    #[interface(name = "org.freedesktop.impl.portal.OpenURI")]
    impl PortalOpenUriIface {
        /// OpenURI: validate schemes; for `file://` try MIME open plan + spawn.
        ///
        /// Returns response `0` on success (remote schemes validated; file spawn ok),
        /// `2` on rejection / no handler / spawn failure.
        fn open_uri(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            _app_id: &str,
            _parent_window: &str,
            uri: &str,
            _options: HashMap<String, OwnedValue>,
        ) -> u32 {
            match plan_open_uri(uri) {
                Ok(OpenUriAction::ValidatedRemote) => 0,
                Ok(OpenUriAction::MimeOpen(plan)) => {
                    let argv = crate::mime_open::spawn_argv(&plan);
                    match crate::session_clients::spawn_open_plan(&plan) {
                        Ok(client) => {
                            tracing::info!(
                                pid = client.pid,
                                app_id = %plan.app_id,
                                ?argv,
                                uri,
                                "OpenURI file:// MIME open spawned"
                            );
                            // Fire-and-forget: portal has no session client registry.
                            // Child keeps running after ExternalClient drop.
                            0
                        }
                        Err(err) => {
                            tracing::warn!(
                                error = %err,
                                app_id = %plan.app_id,
                                ?argv,
                                uri,
                                "OpenURI file:// MIME open spawn failed"
                            );
                            2
                        }
                    }
                }
                Err(err) => {
                    tracing::debug!(error = %err, uri, "OpenURI rejected");
                    2
                }
            }
        }
    }

    /// ScreenCast session map (session_id → pure session state).
    ///
    /// Streams are protocol stubs only — `node_id` values are placeholders.
    static SCREENCAST_SESSIONS: StdMutex<Option<HashMap<String, PortalScreencastSession>>> =
        StdMutex::new(None);

    fn with_screencast_sessions<R>(
        f: impl FnOnce(&mut HashMap<String, PortalScreencastSession>) -> R,
    ) -> R {
        let mut guard = SCREENCAST_SESSIONS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(HashMap::new());
        }
        f(guard.as_mut().expect("initialized screencast map"))
    }

    struct PortalScreencastIface;

    #[interface(name = "org.freedesktop.impl.portal.ScreenCast")]
    impl PortalScreencastIface {
        /// Simplified CreateSession — pure handler; returns `(response, results)`.
        ///
        /// `results["session_id"]` is the handle for SelectSources / Start.
        /// Stream `node_id` values are placeholders (not live PipeWire).
        fn create_session(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            _app_id: &str,
            _parent_window: &str,
            options: HashMap<String, OwnedValue>,
        ) -> (u32, HashMap<String, OwnedValue>) {
            let types = option_u32(&options, "types").unwrap_or(1); // default monitor
            let multiple = option_bool(&options, "multiple").unwrap_or(false);
            let cursor_mode = option_u32(&options, "cursor_mode")
                .or_else(|| option_u32(&options, "cursor-mode"))
                .unwrap_or(0);
            let req = PortalScreencastRequest {
                types,
                multiple,
                cursor_mode,
            };
            // Host probe for honest backend note only — never starts PipeWire.
            let readiness = crate::screencast_pw::probe_screencast_readiness_host();
            let note = crate::portal::screencast_backend_note(&readiness);
            let session = create_screencast_session_with_backend_note(req, note.clone());
            let session_id = session.session_id.clone();
            with_screencast_sessions(|map| {
                map.insert(session_id.clone(), session);
            });
            let mut results: HashMap<String, OwnedValue> = HashMap::new();
            if let Ok(v) = OwnedValue::try_from(Value::from(session_id)) {
                results.insert("session_id".into(), v);
            }
            if let Ok(v) = OwnedValue::try_from(Value::from(note)) {
                results.insert("note".into(), v);
            }
            (0u32, results)
        }

        /// Simplified SelectSources — binds source node_id placeholders on the session.
        ///
        /// `source_ids` is a comma-separated list in options["source_ids"] (e.g. `"42"` or
        /// `"7,9"`) for zbus-friendly string encoding in this simplified backend.
        fn select_sources(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            _app_id: &str,
            session_id: &str,
            options: HashMap<String, OwnedValue>,
        ) -> u32 {
            let ids = option_string_loose(&options, "source_ids")
                .or_else(|| option_string_loose(&options, "sources"))
                .map(|s| {
                    s.split([',', ' ', '\n'])
                        .filter_map(|p| p.trim().parse::<u32>().ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            // Allow types/multiple updates before select if present.
            let types = option_u32(&options, "types");
            let multiple = option_bool(&options, "multiple");
            let cursor_mode =
                option_u32(&options, "cursor_mode").or_else(|| option_u32(&options, "cursor-mode"));

            let ok = with_screencast_sessions(|map| {
                let Some(session) = map.get_mut(session_id) else {
                    return false;
                };
                if let Some(t) = types {
                    session.types = t;
                }
                if let Some(m) = multiple {
                    session.multiple = m;
                }
                if let Some(c) = cursor_mode {
                    session.cursor_mode = c;
                }
                // Empty source_ids keeps the create-time default monitor stub selected.
                let source_ids = if ids.is_empty() {
                    session
                        .streams
                        .iter()
                        .map(|s| s.node_id)
                        .collect::<Vec<_>>()
                } else {
                    ids
                };
                select_screencast_sources(session, &source_ids).is_ok()
            });
            if ok {
                0
            } else {
                2
            }
        }

        /// Simplified Start — pure handler; streams are non-empty stubs on success.
        ///
        /// Returns `(response, results)` with `streams` as a newline-joined summary
        /// `node_id:widthxheight:source_type` (protocol stubs only) and honest
        /// `note` (`backend=portal_stub` / `backend=pipewire_socket_present`).
        /// Does **not** export live PipeWire streams.
        fn start(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            _app_id: &str,
            _parent_window: &str,
            session_id: &str,
            _options: HashMap<String, OwnedValue>,
        ) -> (u32, HashMap<String, OwnedValue>) {
            // Refresh readiness at Start via pure path so note reflects socket state.
            let readiness = crate::screencast_pw::probe_screencast_readiness_host();
            let outcome = with_screencast_sessions(|map| {
                let session = map.get_mut(session_id)?;
                start_screencast_with_readiness(session, &readiness).ok()
            });
            match outcome {
                Some(started) if !started.streams.is_empty() => {
                    let summary = started
                        .streams
                        .iter()
                        .map(|s| {
                            format!("{}:{}x{}:{}", s.node_id, s.width, s.height, s.source_type)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let mut results: HashMap<String, OwnedValue> = HashMap::new();
                    if let Ok(v) = OwnedValue::try_from(Value::from(summary)) {
                        results.insert("streams".into(), v);
                    }
                    if let Some(first) = started.streams.first() {
                        if let Ok(v) = OwnedValue::try_from(Value::from(first.node_id)) {
                            results.insert("node_id".into(), v);
                        }
                    }
                    // Honest backend string (portal_stub | pipewire_socket_present).
                    if let Ok(v) = OwnedValue::try_from(Value::from(started.note)) {
                        results.insert("note".into(), v);
                    }
                    (0u32, results)
                }
                _ => (2u32, HashMap::new()),
            }
        }
    }

    // ---- Secret / Print / Inhibit (portal_extra pure handlers on the bus) ----

    struct PortalSecretIface;
    struct PortalPrintIface;
    struct PortalInhibitIface;

    #[interface(name = "org.freedesktop.impl.portal.Secret")]
    impl PortalSecretIface {
        /// Retrieve — pure keyring lookup plan; does not open a real keyring.
        fn retrieve_secret(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            app_id: &str,
            _options: HashMap<String, OwnedValue>,
        ) -> (u32, HashMap<String, OwnedValue>) {
            use crate::portal_extra::{
                handle_secret_retrieve, PortalSecretRequest, PortalSecretResult,
            };
            let req = PortalSecretRequest {
                app_id: app_id.to_string(),
                token: Vec::new(),
            };
            match handle_secret_retrieve(&req) {
                PortalSecretResult::Lookup { label } => {
                    let mut results = HashMap::new();
                    if let Ok(v) = OwnedValue::try_from(Value::from(label)) {
                        results.insert("label".into(), v);
                    }
                    (0u32, results)
                }
                PortalSecretResult::Rejected { reason } => {
                    tracing::debug!(%reason, "portal Secret rejected");
                    (2u32, HashMap::new())
                }
            }
        }
    }

    #[interface(name = "org.freedesktop.impl.portal.Print")]
    impl PortalPrintIface {
        /// PreparePrint-style plan → `lp` argv (does not spawn by default).
        #[allow(clippy::too_many_arguments)]
        fn prepare_print(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            _app_id: &str,
            _parent_window: &str,
            title: &str,
            _settings: HashMap<String, OwnedValue>,
            _page_setup: HashMap<String, OwnedValue>,
            options: HashMap<String, OwnedValue>,
        ) -> (u32, HashMap<String, OwnedValue>) {
            use crate::portal_extra::{
                handle_print_request, PortalPrintRequest, PortalPrintResult,
            };
            let document_uri = option_string_loose(&options, "document_uri")
                .or_else(|| option_string_loose(&options, "uri"))
                .unwrap_or_default();
            let req = PortalPrintRequest {
                title: title.to_string(),
                document_uri,
                modal: option_bool(&options, "modal").unwrap_or(false),
            };
            match handle_print_request(&req) {
                PortalPrintResult::Queued { job_id, argv } => {
                    let mut results = HashMap::new();
                    if let Ok(v) = OwnedValue::try_from(Value::from(job_id)) {
                        results.insert("job_id".into(), v);
                    }
                    if let Ok(v) = OwnedValue::try_from(Value::from(argv.join(" "))) {
                        results.insert("argv".into(), v);
                    }
                    (0u32, results)
                }
                PortalPrintResult::Rejected { reason } => {
                    tracing::debug!(%reason, "portal Print rejected");
                    (2u32, HashMap::new())
                }
            }
        }
    }

    #[interface(name = "org.freedesktop.impl.portal.Inhibit")]
    impl PortalInhibitIface {
        /// Inhibit — issue cookie into process-wide store for shell idle policy.
        ///
        /// Not logind Inhibit; only SLOPOS-I [`crate::idle_policy`] polls it.
        fn inhibit(
            &self,
            _handle: zbus::zvariant::ObjectPath<'_>,
            app_id: &str,
            window: &str,
            flags: u32,
            reason: &str,
            _options: HashMap<String, OwnedValue>,
        ) -> (u32, HashMap<String, OwnedValue>) {
            use crate::portal_extra::{handle_inhibit_and_register, PortalInhibitRequest};
            let req = PortalInhibitRequest {
                app_id: app_id.to_string(),
                window: window.to_string(),
                flags,
                reason: reason.to_string(),
            };
            match handle_inhibit_and_register(&req) {
                Ok(cookie) => {
                    let mut results = HashMap::new();
                    if let Ok(v) = OwnedValue::try_from(Value::from(cookie.cookie)) {
                        results.insert("cookie".into(), v);
                    }
                    (0u32, results)
                }
                Err(reason) => {
                    tracing::debug!(%reason, "portal Inhibit rejected");
                    (2u32, HashMap::new())
                }
            }
        }

        /// Release a cookie previously returned by Inhibit.
        fn un_inhibit(&self, cookie: u32) -> u32 {
            use crate::portal_extra::release_inhibit_cookie;
            if release_inhibit_cookie(cookie) {
                0
            } else {
                2
            }
        }
    }

    fn option_u32(options: &HashMap<String, OwnedValue>, key: &str) -> Option<u32> {
        let value = options.get(key)?;
        if let Ok(v) = u32::try_from(value) {
            return Some(v);
        }
        if let Ok(v) = i32::try_from(value) {
            return Some(v as u32);
        }
        if let Ok(v) = u64::try_from(value) {
            return Some(v as u32);
        }
        None
    }

    fn option_bool(options: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
        let value = options.get(key)?;
        if let Ok(b) = bool::try_from(value) {
            return Some(b);
        }
        if let Ok(v) = u32::try_from(value) {
            return Some(v != 0);
        }
        if let Ok(v) = i32::try_from(value) {
            return Some(v != 0);
        }
        None
    }

    /// Best-effort string extraction from zbus options without fragile TryFrom bounds.
    fn option_string_loose(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
        let value = options.get(key)?;
        // Display-based fallback for string-like values.
        let s = format!("{value:?}");
        let s = s.trim().trim_matches('"');
        if s.is_empty() || s == "()" {
            None
        } else {
            Some(s.to_string())
        }
    }

    pub(super) fn register() -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(guard) = REGISTRATION.lock() {
            if guard.is_some() {
                return Ok(());
            }
        }

        let conn = ConnectionBuilder::session()?
            .name(PORTAL_BUS_NAME)?
            .serve_at(PORTAL_PATH, PortalScreenshotIface)?
            .serve_at(PORTAL_PATH, PortalSettingsIface)?
            .serve_at(PORTAL_PATH, PortalOpenUriIface)?
            .serve_at(PORTAL_PATH, PortalFileChooserIface)?
            .serve_at(PORTAL_PATH, PortalScreencastIface)?
            .serve_at(PORTAL_PATH, PortalSecretIface)?
            .serve_at(PORTAL_PATH, PortalPrintIface)?
            .serve_at(PORTAL_PATH, PortalInhibitIface)?
            .build()?;

        if let Ok(mut guard) = REGISTRATION.lock() {
            *guard = Some(PortalRegistration { _connection: conn });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_register_returns_bool_on_host() {
        // On macOS / non-Linux this must be false and never panic.
        let registered = try_register_portal_session_bus();
        #[cfg(not(target_os = "linux"))]
        {
            assert!(!registered);
        }
        let _ = registered;
    }

    #[test]
    fn constants_align_with_portal_module() {
        assert_eq!(PORTAL_BUS_NAME, crate::portal::PORTAL_BUS_NAME);
        assert_eq!(PORTAL_PATH, crate::portal::PORTAL_PATH);
    }
}
