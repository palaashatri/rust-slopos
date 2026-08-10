//! Standard xdg-desktop-portal session-bus export (Linux only).
//!
//! The service deliberately exposes the frontend-facing portal name, path, and
//! method signatures. Requests that need a user dialog or another privileged
//! service return a real [`org.freedesktop.portal.Request`] object and complete
//! it with a `Response` signal. Until an authoritative backend is attached,
//! the response is `2` (failed) rather than a fabricated path, setting,
//! selection, URI launch, or PipeWire node.

use crate::portal::{
    PORTAL_BUS_NAME, PORTAL_FILECHOOSER_INTERFACE, PORTAL_OPENURI_INTERFACE, PORTAL_PATH,
    PORTAL_REQUEST_INTERFACE, PORTAL_SCREENCAST_INTERFACE, PORTAL_SCREENSHOT_INTERFACE,
    PORTAL_SETTINGS_INTERFACE,
};

/// Register the standard portal interfaces on the Linux session bus.
///
/// This is best effort so a missing session bus or an already-running portal
/// cannot prevent the shell from starting. `true` means that SLOPOS acquired
/// the standard well-known name and installed its interfaces; it does not mean
/// every portal backend is available.
pub fn try_register_portal_session_bus() -> bool {
    #[cfg(target_os = "linux")]
    {
        match linux::register() {
            Ok(()) => {
                tracing::info!(
                    bus = PORTAL_BUS_NAME,
                    path = PORTAL_PATH,
                    request = PORTAL_REQUEST_INTERFACE,
                    screenshot = PORTAL_SCREENSHOT_INTERFACE,
                    settings = PORTAL_SETTINGS_INTERFACE,
                    openuri = PORTAL_OPENURI_INTERFACE,
                    filechooser = PORTAL_FILECHOOSER_INTERFACE,
                    screencast = PORTAL_SCREENCAST_INTERFACE,
                    "standard SLOPOS-I portal interfaces registered on session bus"
                );
                true
            }
            Err(err) => {
                tracing::warn!(error = %err, "standard SLOPOS-I portal registration skipped");
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

/// Build a request object path following the xdg-desktop-portal convention.
///
/// Callers can subscribe to `org.freedesktop.portal.Request::Response` before
/// invoking a portal method because the sender and `handle_token` are part of
/// the returned path. The helper is kept platform independent for unit tests.
pub(crate) fn request_path_for_sender(sender: &str, token: Option<&str>) -> Result<String, String> {
    let sender_component = sender
        .trim_start_matches(':')
        .chars()
        .map(|ch| match ch {
            '.' | ':' => '_',
            c if c.is_ascii_alphanumeric() || c == '_' => c,
            _ => '_',
        })
        .collect::<String>();
    if sender_component.is_empty() {
        return Err("D-Bus request sender is empty".into());
    }

    let token = token.unwrap_or("slopos-request");
    if token.is_empty()
        || !token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(format!("invalid portal handle_token: {token:?}"));
    }

    Ok(format!("{PORTAL_PATH}/request/{sender_component}/{token}"))
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex as StdMutex;

    use crate::portal::{
        handle_file_chooser_open, handle_file_chooser_save, plan_open_uri,
        portal_screenshot_uri_for, take_portal_style_screenshot_with, OpenUriAction,
        PortalFileChooserRequest, PortalScreenshotRequest,
    };
    use zbus::blocking::connection::Builder as ConnectionBuilder;
    use zbus::blocking::Connection as BlockingConnection;
    use zbus::interface;
    use zbus::message::Header;
    use zbus::object_server::{ObjectServer, SignalContext};
    use zbus::zvariant::{ObjectPath, OwnedFd, OwnedObjectPath, OwnedValue, Value};
    use zbus::{fdo, Connection};

    /// Keeps the blocking session-bus connection alive for the shell lifetime.
    static REGISTRATION: StdMutex<Option<PortalRegistration>> = StdMutex::new(None);
    static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct PortalRegistration {
        _connection: BlockingConnection,
    }

    /// Shared request object required by every portal method involving user
    /// interaction. `Close` removes the object; a completed request may also
    /// be removed by a future backend once its response has been delivered.
    struct PortalRequestIface;

    #[interface(name = "org.freedesktop.portal.Request")]
    impl PortalRequestIface {
        async fn close(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(object_server)] server: &ObjectServer,
        ) -> fdo::Result<()> {
            let path = header
                .path()
                .ok_or_else(|| fdo::Error::Failed("request has no object path".into()))?;
            server
                .remove::<PortalRequestIface, _>(path)
                .await
                .map_err(|err| {
                    fdo::Error::Failed(format!("failed to close portal request: {err}"))
                })?;
            Ok(())
        }

        #[zbus(signal)]
        async fn response(
            signal_context: &SignalContext<'_>,
            response: u32,
            results: &HashMap<String, OwnedValue>,
        ) -> zbus::Result<()>;
    }

    /// Install a Request object, emit its terminal response, and return its
    /// object path to the caller. Emitting before the method reply is safe for
    /// clients because the path includes their sender/token by convention.
    async fn complete_request(
        header: &Header<'_>,
        options: &HashMap<String, OwnedValue>,
        server: &ObjectServer,
        connection: &Connection,
        response: u32,
        results: HashMap<String, OwnedValue>,
    ) -> fdo::Result<OwnedObjectPath> {
        let sender = header
            .sender()
            .map(ToString::to_string)
            .unwrap_or_else(|| "unknown".into());
        let token = option_string_loose(options, "handle_token");
        let token = token.unwrap_or_else(|| {
            format!("slopos-{}", REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed))
        });
        let path =
            request_path_for_sender(&sender, Some(&token)).map_err(fdo::Error::InvalidArgs)?;
        let path = OwnedObjectPath::try_from(path)
            .map_err(|err| fdo::Error::InvalidArgs(format!("invalid request path: {err}")))?;

        let inserted = server
            .at(path.clone(), PortalRequestIface)
            .await
            .map_err(|err| fdo::Error::Failed(format!("failed to export portal request: {err}")))?;
        if !inserted {
            return Err(fdo::Error::Failed(
                "portal handle_token is already in use".into(),
            ));
        }

        let signal_context = SignalContext::new(connection, path.clone()).map_err(|err| {
            fdo::Error::Failed(format!("failed to create response context: {err}"))
        })?;
        PortalRequestIface::response(&signal_context, response, &results)
            .await
            .map_err(|err| fdo::Error::Failed(format!("failed to emit portal response: {err}")))?;
        Ok(path)
    }

    fn error_response() -> HashMap<String, OwnedValue> {
        HashMap::new()
    }

    fn string_result(key: &str, value: impl Into<String>) -> HashMap<String, OwnedValue> {
        let mut results = HashMap::new();
        if let Ok(value) = OwnedValue::try_from(Value::from(value.into())) {
            results.insert(key.into(), value);
        }
        results
    }

    fn uri_results(uris: &[String]) -> HashMap<String, OwnedValue> {
        let mut results = HashMap::new();
        if let Ok(value) = OwnedValue::try_from(Value::from(uris.to_vec())) {
            results.insert("uris".into(), value);
        }
        results
    }

    struct PortalScreenshotIface;

    #[interface(name = "org.freedesktop.portal.Screenshot")]
    impl PortalScreenshotIface {
        #[zbus(property)]
        fn version(&self) -> u32 {
            3
        }

        #[zbus(property, name = "AvailableTargets")]
        fn available_targets(&self) -> u32 {
            // Only whole-output capture is potentially available; window and
            // area picking are not advertised until those targets are real.
            1
        }

        #[zbus(out_args("handle"))]
        async fn screenshot(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            _parent_window: &str,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            let request = PortalScreenshotRequest {
                interactive: option_bool(&options, "interactive").unwrap_or(false),
                include_cursor: option_bool(&options, "cursor").unwrap_or(false),
            };
            let (response, results) = match take_portal_style_screenshot_with(request) {
                Ok(result) => (
                    0,
                    string_result("uri", portal_screenshot_uri_for(&result.path)),
                ),
                Err(error) => {
                    tracing::warn!(error = %error, "standard Screenshot request failed closed");
                    (2, error_response())
                }
            };
            complete_request(&header, &options, server, connection, response, results).await
        }
    }

    struct PortalSettingsIface;

    #[interface(name = "org.freedesktop.portal.Settings")]
    impl PortalSettingsIface {
        #[zbus(property)]
        fn version(&self) -> u32 {
            2
        }

        fn read(&self, _namespace: &str, _key: &str) -> fdo::Result<OwnedValue> {
            Err(fdo::Error::NotSupported(
                "SLOPOS Settings has no authoritative backend".into(),
            ))
        }

        fn read_all(
            &self,
            _namespaces: Vec<String>,
        ) -> fdo::Result<HashMap<String, HashMap<String, OwnedValue>>> {
            Err(fdo::Error::NotSupported(
                "SLOPOS Settings has no authoritative backend".into(),
            ))
        }
    }

    struct PortalFileChooserIface;

    #[interface(name = "org.freedesktop.portal.FileChooser")]
    impl PortalFileChooserIface {
        #[zbus(property)]
        fn version(&self) -> u32 {
            4
        }

        #[zbus(out_args("handle"))]
        async fn open_file(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            parent_window: &str,
            title: &str,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            let (response, results) =
                synthetic_file_open_if_enabled(parent_window, title, &options);
            complete_request(&header, &options, server, connection, response, results).await
        }

        #[zbus(out_args("handle"))]
        async fn save_file(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            parent_window: &str,
            title: &str,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            let (response, results) =
                synthetic_file_save_if_enabled(parent_window, title, &options);
            complete_request(&header, &options, server, connection, response, results).await
        }
    }

    fn synthetic_file_open_if_enabled(
        _parent_window: &str,
        title: &str,
        options: &HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        if std::env::var("SLOPOS_PORTAL_ALLOW_SYNTHETIC_SELECTION")
            .ok()
            .as_deref()
            != Some("1")
        {
            tracing::warn!("standard FileChooser OpenFile failed closed: no interactive chooser");
            return (2, error_response());
        }
        let req = PortalFileChooserRequest {
            title: title.into(),
            multiple: option_bool(options, "multiple").unwrap_or(false),
            directory: option_bool(options, "directory").unwrap_or(false),
            current_folder: option_string_loose(options, "current_folder"),
            ..Default::default()
        };
        let names = option_string_loose(options, "selected")
            .map(|selected| {
                selected
                    .split(['\n', ','])
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        match handle_file_chooser_open(&req, &refs) {
            Ok(result) if !result.uris.is_empty() => (0, uri_results(&result.uris)),
            Ok(_) => (1, error_response()),
            Err(error) => {
                tracing::warn!(%error, "standard FileChooser OpenFile failed");
                (2, error_response())
            }
        }
    }

    fn synthetic_file_save_if_enabled(
        _parent_window: &str,
        title: &str,
        options: &HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        if std::env::var("SLOPOS_PORTAL_ALLOW_SYNTHETIC_SELECTION")
            .ok()
            .as_deref()
            != Some("1")
        {
            tracing::warn!("standard FileChooser SaveFile failed closed: no interactive chooser");
            return (2, error_response());
        }
        let req = PortalFileChooserRequest {
            title: title.into(),
            current_folder: option_string_loose(options, "current_folder"),
            current_name: option_string_loose(options, "current_name"),
            ..Default::default()
        };
        match handle_file_chooser_save(&req, option_bool(options, "confirm").unwrap_or(true)) {
            Ok(result) if !result.uris.is_empty() => (0, uri_results(&result.uris)),
            Ok(_) => (1, error_response()),
            Err(error) => {
                tracing::warn!(%error, "standard FileChooser SaveFile failed");
                (2, error_response())
            }
        }
    }

    struct PortalOpenUriIface;

    #[interface(name = "org.freedesktop.portal.OpenURI")]
    impl PortalOpenUriIface {
        #[zbus(property)]
        fn version(&self) -> u32 {
            5
        }

        #[zbus(out_args("handle"))]
        async fn open_uri(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            _parent_window: &str,
            uri: &str,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            // The standard OpenURI method does not accept raw file:// URIs;
            // use FileChooser/OpenFile for document access instead.
            let response = if uri.starts_with("file:") {
                tracing::warn!(uri, "standard OpenURI rejects file URI");
                2
            } else {
                match plan_open_uri(uri) {
                    Ok(OpenUriAction::ValidatedRemote) => {
                        tracing::warn!(uri, "standard OpenURI has no authoritative launcher");
                        2
                    }
                    Ok(OpenUriAction::MimeOpen(_)) | Err(_) => 2,
                }
            };
            complete_request(
                &header,
                &options,
                server,
                connection,
                response,
                error_response(),
            )
            .await
        }

        #[zbus(out_args("supported"))]
        fn scheme_supported(&self, _scheme: &str, _options: HashMap<String, OwnedValue>) -> bool {
            // Advertising support would be a promise to launch a handler. No
            // such authoritative launcher is connected yet.
            false
        }

        #[zbus(out_args("handle"))]
        async fn open_file(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            _fd: OwnedFd,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            tracing::warn!(
                "standard OpenURI OpenFile failed closed: document/launcher backend absent"
            );
            complete_request(&header, &options, server, connection, 2, error_response()).await
        }

        #[zbus(out_args("handle"))]
        async fn open_directory(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            _fd: OwnedFd,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            tracing::warn!(
                "standard OpenURI OpenDirectory failed closed: file manager backend absent"
            );
            complete_request(&header, &options, server, connection, 2, error_response()).await
        }
    }

    struct PortalScreenCastIface;

    #[interface(name = "org.freedesktop.portal.ScreenCast")]
    impl PortalScreenCastIface {
        #[zbus(property)]
        fn version(&self) -> u32 {
            6
        }

        #[zbus(property, name = "AvailableSourceTypes")]
        fn available_source_types(&self) -> u32 {
            0
        }

        #[zbus(property, name = "AvailableCursorModes")]
        fn available_cursor_modes(&self) -> u32 {
            0
        }

        #[zbus(out_args("handle"))]
        async fn create_session(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            let readiness = crate::screencast_pw::probe_screencast_readiness_host();
            tracing::warn!(
                backend = readiness.backend.as_str(),
                socket = readiness.pipewire_socket_present,
                "standard ScreenCast CreateSession failed closed: no live PipeWire export"
            );
            complete_request(&header, &options, server, connection, 2, error_response()).await
        }

        #[zbus(out_args("handle"))]
        async fn select_sources(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            _session_handle: ObjectPath<'_>,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            tracing::warn!("standard ScreenCast SelectSources failed closed: no permission graph");
            complete_request(&header, &options, server, connection, 2, error_response()).await
        }

        #[zbus(out_args("handle"))]
        async fn start(
            &self,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] connection: &Connection,
            #[zbus(object_server)] server: &ObjectServer,
            _session_handle: ObjectPath<'_>,
            _parent_window: &str,
            options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedObjectPath> {
            tracing::warn!("standard ScreenCast Start failed closed: no live PipeWire stream");
            complete_request(&header, &options, server, connection, 2, error_response()).await
        }

        #[zbus(out_args("fd"))]
        fn open_pipe_wire_remote(
            &self,
            _session_handle: ObjectPath<'_>,
            _options: HashMap<String, OwnedValue>,
        ) -> fdo::Result<OwnedFd> {
            Err(fdo::Error::NotSupported(
                "SLOPOS ScreenCast has no live PipeWire remote".into(),
            ))
        }
    }

    fn option_u32(options: &HashMap<String, OwnedValue>, key: &str) -> Option<u32> {
        let value = options.get(key)?;
        u32::try_from(value)
            .ok()
            .or_else(|| i32::try_from(value).ok().map(|v| v as u32))
            .or_else(|| u64::try_from(value).ok().map(|v| v as u32))
    }

    fn option_bool(options: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
        let value = options.get(key)?;
        bool::try_from(value)
            .ok()
            .or_else(|| option_u32(options, key).map(|v| v != 0))
    }

    /// Extract a string option without assuming a particular zvariant borrow
    /// lifetime. This is only used by the explicit synthetic test backend;
    /// normal portal calls fail closed before selection is attempted.
    fn option_string_loose(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
        let value = options.get(key)?;
        let value = <&str>::try_from(value).ok()?.trim();
        (!value.is_empty()).then(|| value.to_owned())
    }

    pub(super) fn register() -> Result<(), Box<dyn std::error::Error>> {
        if REGISTRATION
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
        {
            return Ok(());
        }

        let connection = ConnectionBuilder::session()?
            .name(PORTAL_BUS_NAME)?
            .serve_at(PORTAL_PATH, PortalScreenshotIface)?
            .serve_at(PORTAL_PATH, PortalSettingsIface)?
            .serve_at(PORTAL_PATH, PortalFileChooserIface)?
            .serve_at(PORTAL_PATH, PortalOpenUriIface)?
            .serve_at(PORTAL_PATH, PortalScreenCastIface)?
            .build()?;

        if let Ok(mut guard) = REGISTRATION.lock() {
            *guard = Some(PortalRegistration {
                _connection: connection,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_path_follows_standard_sender_token_convention() {
        assert_eq!(
            request_path_for_sender(":1.42", Some("gtk_7")).unwrap(),
            "/org/freedesktop/portal/desktop/request/1_42/gtk_7"
        );
    }

    #[test]
    fn request_path_rejects_invalid_token() {
        let error = request_path_for_sender(":1.42", Some("bad/token")).unwrap_err();
        assert!(error.contains("handle_token"));
    }

    #[test]
    fn standard_portal_constants_are_not_private_impl_names() {
        assert_eq!(PORTAL_BUS_NAME, "org.freedesktop.portal.Desktop");
        assert_eq!(PORTAL_PATH, "/org/freedesktop/portal/desktop");
        for interface in [
            PORTAL_REQUEST_INTERFACE,
            PORTAL_SCREENSHOT_INTERFACE,
            PORTAL_SETTINGS_INTERFACE,
            PORTAL_OPENURI_INTERFACE,
            PORTAL_FILECHOOSER_INTERFACE,
            PORTAL_SCREENCAST_INTERFACE,
        ] {
            assert!(interface.starts_with("org.freedesktop.portal."));
            assert!(!interface.contains(".impl."));
        }
    }

    #[test]
    fn try_register_is_best_effort_and_never_panics() {
        let _ = try_register_portal_session_bus();
    }
}
