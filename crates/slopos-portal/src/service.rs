//! The backend side of the file chooser portal.
//!
//! xdg-desktop-portal is the frontend: it owns `org.freedesktop.portal.Desktop`
//! and receives applications' `org.freedesktop.portal.FileChooser` calls. It
//! forwards each call to the backend that `portals.conf` selects for the
//! desktop. SLOPOS is such a backend. It owns the well-known name
//! `org.freedesktop.impl.portal.desktop.slopos` and serves the backend
//! interface `org.freedesktop.impl.portal.FileChooser` at
//! `/org/freedesktop/portal/desktop`. It never claims the frontend name.
//!
//! What the backend specification says
//! (flatpak.github.io/xdg-desktop-portal, doc-org.freedesktop.impl.portal.FileChooser
//! and doc-org.freedesktop.impl.portal.Request):
//! - Each method takes `handle`, described as "Object path for the Request
//!   object representing this call". The frontend chooses that path.
//! - The backend exports `org.freedesktop.impl.portal.Request` at that path
//!   "when handling a method call", and emits its `Created` signal, which has
//!   no arguments, once the object "has been exported and is ready to receive
//!   calls".
//! - The methods have `OUT response u` and `OUT results a{sv}` arguments. The
//!   answer is therefore the method reply, sent when the user has answered.
//! - `Close()` on the Request "ends the user interaction", and dialogs
//!   related to the request should be closed. The Request interface has no
//!   Response signal.
//!
//! So a call here exports the Request, starts the dialog program, and replies
//! with `(response, results)` once the program has finished. The Request is
//! unexported before the reply is sent. A `Close` that arrives while the
//! dialog is open stops the program, and the call then replies with response 1.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use zbus::zvariant::{OwnedObjectPath, Value};
use zbus::{fdo, interface, Connection, SignalContext};

use crate::backend::Backend;
use crate::request::{parse_options, OptionMap};
use crate::response::{response_code, uris, CANCELLED};
use crate::spec::{ChooserReply, Kind};

/// The well-known name SLOPOS owns on the session bus.
pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.slopos";
/// The object path of the backend interfaces, as the specification names it.
pub const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
/// The backend interface for file choosers (not the frontend's
/// `org.freedesktop.portal.FileChooser`).
pub const FILE_CHOOSER_INTERFACE: &str = "org.freedesktop.impl.portal.FileChooser";
/// The backend interface every exported Request implements.
pub const REQUEST_INTERFACE: &str = "org.freedesktop.impl.portal.Request";

/// A method reply: the `response` code and the `results` vardict.
pub type Answer = (u32, HashMap<&'static str, Value<'static>>);

/// State shared by a Request object and the thread that runs its chooser.
#[derive(Debug, Default)]
pub struct RequestState {
    closed: AtomicBool,
    /// Process id of the dialog program, 0 when none is running.
    child: AtomicU32,
}

impl RequestState {
    /// Record the dialog program's pid. If `close` already ran, the program
    /// is stopped here. `close` sets `closed` before it reads the pid, so
    /// whichever side runs second sees the other's write.
    fn attach(&self, pid: u32) {
        self.child.store(pid, Ordering::SeqCst);
        if self.closed.load(Ordering::SeqCst) {
            terminate(pid);
        }
    }

    /// The program has been waited for, so its pid is no longer signalled.
    fn detach(&self) {
        self.child.store(0, Ordering::SeqCst);
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let pid = self.child.load(Ordering::SeqCst);
        if pid != 0 {
            terminate(pid);
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

fn terminate(pid: u32) {
    // SAFETY: kill(2) takes a process id and a signal and touches no Rust state.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
}

/// `org.freedesktop.impl.portal.Request`, exported at the handle of one call
/// for as long as the user interaction lasts.
pub struct RequestObject {
    state: Arc<RequestState>,
}

#[interface(name = "org.freedesktop.impl.portal.Request")]
impl RequestObject {
    /// Ends the user interaction: the dialog program is stopped and the call
    /// that created this object replies with response 1.
    async fn close(&self) {
        self.state.close();
    }

    /// Emitted once the object is exported and ready to receive calls.
    #[zbus(signal)]
    async fn created(ctxt: &SignalContext<'_>) -> zbus::Result<()>;
}

/// The most file chooser requests that run at once. Each one holds a thread
/// and a dialog process, so a caller cannot start more than this; the next
/// call is refused with `LimitsExceeded` until one finishes.
pub const MAX_ACTIVE_REQUESTS: usize = 4;

/// One running request's place in the count of active requests. It is
/// released when the request is dropped, whatever way the call ends.
#[derive(Debug)]
pub struct RequestSlot(Arc<AtomicUsize>);

impl RequestSlot {
    /// Take a slot, or `None` when [`MAX_ACTIVE_REQUESTS`] are already running.
    pub fn reserve(active: &Arc<AtomicUsize>) -> Option<Self> {
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIVE_REQUESTS {
            active.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self(active.clone()))
    }
}

impl Drop for RequestSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `org.freedesktop.impl.portal.FileChooser` for one backend.
pub struct FileChooserPortal {
    backend: Arc<dyn Backend>,
    active: Arc<AtomicUsize>,
}

impl FileChooserPortal {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self {
            backend,
            active: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Export the Request at `handle`, run the chooser on its own thread and
    /// wait for it. The executor is not blocked while the dialog is open, so
    /// `Close` on the Request is served in the meantime.
    async fn answer(
        &self,
        connection: &Connection,
        handle: &OwnedObjectPath,
        kind: Kind,
        title: &str,
        options: &OptionMap<'_>,
    ) -> fdo::Result<Answer> {
        let _slot = RequestSlot::reserve(&self.active).ok_or_else(|| {
            fdo::Error::LimitsExceeded("too many file chooser requests are running".into())
        })?;
        let spec = parse_options(kind, title, options).map_err(fdo::Error::InvalidArgs)?;
        let state = Arc::new(RequestState::default());
        let object_server = connection.object_server();
        let added = object_server
            .at(
                handle.as_ref(),
                RequestObject {
                    state: state.clone(),
                },
            )
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))?;
        if !added {
            return Err(fdo::Error::Failed(format!(
                "a request already exists at {handle}"
            )));
        }
        if let Ok(request) = object_server
            .interface::<_, RequestObject>(handle.as_ref())
            .await
        {
            let _ = RequestObject::created(request.signal_context()).await;
        }
        let (sender, receiver) = futures::channel::oneshot::channel();
        let backend = self.backend.clone();
        let worker = state.clone();
        let spawned = std::thread::Builder::new()
            .name("portal-request".into())
            .spawn(move || {
                let reply = backend.choose(&spec, &|pid| worker.attach(pid));
                worker.detach();
                let _ = sender.send(reply);
            });
        if let Err(error) = spawned {
            self.unexport(connection, handle).await;
            return Err(fdo::Error::Failed(error.to_string()));
        }
        let reply = receiver.await.unwrap_or_else(|_| ChooserReply::Failed {
            message: "the file chooser thread ended without a reply".into(),
        });
        self.unexport(connection, handle).await;
        Ok(answer_for(&reply, state.is_closed()))
    }

    async fn unexport(&self, connection: &Connection, handle: &OwnedObjectPath) {
        let _ = connection
            .object_server()
            .remove::<RequestObject, _>(handle.as_ref())
            .await;
    }
}

/// The method reply for a finished chooser. A closed request answers
/// cancelled, unless the dialog had already produced a choice.
pub fn answer_for(reply: &ChooserReply, closed: bool) -> Answer {
    if closed && !matches!(reply, ChooserReply::Chosen { .. }) {
        return (CANCELLED, HashMap::new());
    }
    let mut results = HashMap::new();
    if let ChooserReply::Chosen { paths } = reply {
        results.insert("uris", Value::from(uris(paths)));
    }
    (response_code(reply), results)
}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooserPortal {
    /// Show an open dialog. The reply carries the response and `uris`.
    async fn open_file(
        &self,
        #[zbus(connection)] connection: &Connection,
        handle: OwnedObjectPath,
        app_id: &str,
        parent_window: &str,
        title: &str,
        options: OptionMap<'_>,
    ) -> fdo::Result<Answer> {
        log::debug!("OpenFile for {app_id:?} from window {parent_window:?}");
        self.answer(connection, &handle, Kind::Open, title, &options)
            .await
    }

    /// Show a save dialog. The reply carries the response and `uris`.
    async fn save_file(
        &self,
        #[zbus(connection)] connection: &Connection,
        handle: OwnedObjectPath,
        app_id: &str,
        parent_window: &str,
        title: &str,
        options: OptionMap<'_>,
    ) -> fdo::Result<Answer> {
        log::debug!("SaveFile for {app_id:?} from window {parent_window:?}");
        self.answer(connection, &handle, Kind::Save, title, &options)
            .await
    }

    /// Choose the folder the files are saved into; the answer is that folder.
    async fn save_files(
        &self,
        #[zbus(connection)] connection: &Connection,
        handle: OwnedObjectPath,
        app_id: &str,
        parent_window: &str,
        title: &str,
        options: OptionMap<'_>,
    ) -> fdo::Result<Answer> {
        log::debug!("SaveFiles for {app_id:?} from window {parent_window:?}");
        self.answer(connection, &handle, Kind::OpenFolder, title, &options)
            .await
    }
}

/// Register the backend on the session bus and serve it. The returned
/// connection keeps the service alive.
pub fn serve(backend: Arc<dyn Backend>) -> zbus::Result<Connection> {
    zbus::block_on(async {
        zbus::connection::Builder::session()?
            .name(BUS_NAME)?
            .serve_at(OBJECT_PATH, FileChooserPortal::new(backend))?
            .build()
            .await
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::process::launch_captured;
    use std::path::PathBuf;

    #[test]
    fn a_finished_chooser_maps_onto_the_response_and_results() {
        let chosen = ChooserReply::Chosen {
            paths: vec![PathBuf::from("/a b")],
        };
        let (code, results) = answer_for(&chosen, false);
        assert_eq!(code, 0);
        let uris = Vec::<String>::try_from(results["uris"].try_clone().unwrap()).unwrap();
        assert_eq!(uris, ["file:///a%20b"]);
        assert_eq!(answer_for(&ChooserReply::Cancelled, false).0, CANCELLED);
        let (code, results) = answer_for(
            &ChooserReply::Failed {
                message: "boom".into(),
            },
            false,
        );
        assert_eq!(code, 2);
        assert!(results.is_empty());
    }

    #[test]
    fn a_closed_request_answers_cancelled_unless_a_choice_was_made() {
        let chosen = ChooserReply::Chosen {
            paths: vec![PathBuf::from("/a")],
        };
        assert_eq!(
            answer_for(&chosen, true).0,
            0,
            "a choice made before Close stands"
        );
        let killed = ChooserReply::Failed {
            message: "stopped".into(),
        };
        assert_eq!(answer_for(&killed, true).0, CANCELLED);
        assert!(answer_for(&killed, true).1.is_empty());
    }

    #[test]
    fn close_stops_the_dialog_program_whether_it_comes_before_or_after_start() {
        let args: &[&str] = &["-c", "exec sleep 30"];
        let candidates: &[(&str, &[&str])] = &[("/bin/sh", args)];

        // Close while the program runs.
        let child = launch_captured(candidates).expect("sh is installed");
        let state = RequestState::default();
        state.attach(child.id());
        state.close();
        let output = child.wait_with_output().expect("wait");
        assert!(!output.status.success(), "the program was stopped");

        // Close before the program has been attached: attach stops it.
        let child = launch_captured(candidates).expect("sh is installed");
        let state = RequestState::default();
        state.close();
        state.attach(child.id());
        let output = child.wait_with_output().expect("wait");
        assert!(!output.status.success(), "a late attach is stopped at once");
        assert!(state.is_closed());
    }

    #[test]
    fn requests_are_bounded_and_a_finished_request_frees_its_slot() {
        let active = Arc::new(AtomicUsize::new(0));
        let slots: Vec<RequestSlot> = (0..MAX_ACTIVE_REQUESTS)
            .map(|_| RequestSlot::reserve(&active).expect("a slot below the bound"))
            .collect();
        assert!(
            RequestSlot::reserve(&active).is_none(),
            "a request past the bound is refused"
        );
        assert_eq!(active.load(Ordering::SeqCst), MAX_ACTIVE_REQUESTS);
        drop(slots);
        assert_eq!(active.load(Ordering::SeqCst), 0, "every slot is released");
        assert!(RequestSlot::reserve(&active).is_some());
    }
}
