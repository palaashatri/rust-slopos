//! Integration test on a session bus: the real `slopos-portal` binary runs in
//! `--test-first-file` mode and serves the backend interface
//! `org.freedesktop.impl.portal.FileChooser` under
//! `org.freedesktop.impl.portal.desktop.slopos`.
//!
//! The client acts as the xdg-desktop-portal frontend does: it chooses the
//! handle, subscribes to the Request's `Created` signal (emitted before the
//! call answers), calls the method, and reads `(response, results)` from the
//! method reply. It then checks that the Request is gone once the call has
//! answered.
//!
//! Needs a session bus. Run it as
//! `dbus-run-session -- cargo test -p slopos-portal --test portal_bus`.
//! Without a bus the test prints NOT_RUN and returns; that is not a pass.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use slopos_portal::response::file_uri;
use zbus::blocking::{fdo::DBusProxy, Connection, Proxy};
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const BUS: &str = "org.freedesktop.impl.portal.desktop.slopos";
const FRONTEND_BUS: &str = "org.freedesktop.portal.Desktop";
const OBJECT: &str = "/org/freedesktop/portal/desktop";
const FILE_CHOOSER: &str = "org.freedesktop.impl.portal.FileChooser";
const REQUEST: &str = "org.freedesktop.impl.portal.Request";
const WAIT: Duration = Duration::from_secs(20);

/// The portal process, killed when the test ends.
struct Service(Child);

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Start the backend in test `mode` and wait for its "ready" line (written
/// after the bus name is owned).
fn start_service(mode: &str) -> Service {
    let mut child = Command::new(env!("CARGO_BIN_EXE_slopos-portal"))
        .arg(mode)
        .stdout(Stdio::piped())
        .spawn()
        .expect("start slopos-portal");
    let stdout = child.stdout.take().expect("piped stdout");
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .expect("read the ready line");
    assert_eq!(line.trim(), "slopos-portal: ready");
    Service(child)
}

fn nul_terminated(path: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let mut bytes = path.as_os_str().as_bytes().to_vec();
    bytes.push(0);
    bytes
}

/// The method reply: `(response, results)`.
type Answer = (u32, HashMap<String, OwnedValue>);

/// Run `f` on a thread and wait for its result, bounded by [`WAIT`] so a
/// broken service fails the test instead of hanging it. Every D-Bus call in
/// this test goes through here.
fn within_wait<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(f());
    });
    receiver
        .recv_timeout(WAIT)
        .expect("the backend answered within the timeout")
}

/// Call a backend method with the handle the frontend would have chosen. The
/// call runs under [`within_wait`].
fn call(
    conn: &Connection,
    method: &'static str,
    handle: &'static str,
    title: &'static str,
    options: HashMap<&'static str, Value<'static>>,
) -> zbus::Result<Answer> {
    let conn = conn.clone();
    within_wait(move || -> zbus::Result<Answer> {
        let handle = ObjectPath::try_from(handle)?;
        let reply = conn.call_method(
            Some(BUS),
            OBJECT,
            Some(FILE_CHOOSER),
            method,
            &(handle, "", "", title, options),
        )?;
        reply.body().deserialize::<Answer>()
    })
}

/// Subscribe to the Request object at `handle` before the call that exports
/// it, so the `Created` signal is not missed. The match rule is added on the
/// bus, so it runs under [`within_wait`] too.
fn subscribe(conn: &Connection, handle: &str) -> Proxy<'static> {
    let conn = conn.clone();
    let handle = handle.to_owned();
    within_wait(move || Proxy::new(&conn, BUS, handle, REQUEST).expect("Request proxy"))
}

fn uris_of(results: &HashMap<String, OwnedValue>) -> Vec<String> {
    Vec::<String>::try_from(results["uris"].try_clone().unwrap()).unwrap()
}

#[test]
fn file_chooser_backend_answers_through_the_request_object_on_the_bus() {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("NOT_RUN: no session bus; run with dbus-run-session -- cargo test -p slopos-portal --test portal_bus");
        return;
    }
    let folder = tempfile::tempdir().expect("temp folder");
    std::fs::write(folder.path().join("b.txt"), b"x").unwrap();
    std::fs::write(folder.path().join("a.png"), b"x").unwrap();
    let empty = tempfile::tempdir().expect("empty folder");

    let service = start_service("--test-first-file");
    let conn = Connection::session().expect("session bus");

    // The backend owns its own name and leaves the frontend's name alone.
    let bus = conn.clone();
    let names: Vec<String> = within_wait(move || {
        DBusProxy::new(&bus)
            .expect("bus proxy")
            .list_names()
            .expect("list names")
            .into_iter()
            .map(|name| name.to_string())
            .collect()
    });
    assert!(
        names.iter().any(|name| name == BUS),
        "the backend name is owned"
    );
    assert!(
        !names.iter().any(|name| name == FRONTEND_BUS),
        "the backend must not claim the frontend name"
    );

    // OpenFile: the test backend picks the first file (directories first,
    // then by name), so a.png is chosen. The reply is the answer.
    let handle = "/org/freedesktop/portal/desktop/request/xdgtest/open_token";
    let request = subscribe(&conn, handle);
    let subscribed = request.clone();
    let created = within_wait(move || subscribed.receive_signal("Created").expect("subscribe"));
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("current_folder", Value::from(nul_terminated(folder.path())));
    let (code, results) = call(&conn, "OpenFile", handle, "Open", options).expect("OpenFile");
    assert!(
        within_wait(move || {
            let mut created = created;
            created.next().is_some()
        }),
        "the Request was announced with Created"
    );
    assert_eq!(code, 0, "success");
    assert_eq!(
        uris_of(&results),
        vec![file_uri(&folder.path().join("a.png")).unwrap()]
    );
    let close = request.clone();
    assert!(
        within_wait(move || close.call_method("Close", &()).is_err()),
        "the Request is unexported once the call has answered"
    );

    // SaveFile: the suggested name is joined onto the folder.
    let handle = "/org/freedesktop/portal/desktop/request/xdgtest/save_token";
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("current_folder", Value::from(nul_terminated(folder.path())));
    options.insert("current_name", Value::from("new.txt"));
    let (code, results) = call(&conn, "SaveFile", handle, "Save", options).expect("SaveFile");
    assert_eq!(code, 0);
    assert_eq!(
        uris_of(&results),
        vec![file_uri(&folder.path().join("new.txt")).unwrap()]
    );

    // SaveFiles answers with the folder itself.
    let handle = "/org/freedesktop/portal/desktop/request/xdgtest/folder_token";
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("current_folder", Value::from(nul_terminated(folder.path())));
    let (code, results) = call(&conn, "SaveFiles", handle, "Folder", options).expect("SaveFiles");
    assert_eq!(code, 0);
    assert_eq!(uris_of(&results), vec![file_uri(folder.path()).unwrap()]);

    // An empty folder has nothing to pick: the other-failure response code,
    // and no results.
    let handle = "/org/freedesktop/portal/desktop/request/xdgtest/empty_token";
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("current_folder", Value::from(nul_terminated(empty.path())));
    let (code, results) = call(&conn, "OpenFile", handle, "Open", options).expect("OpenFile");
    assert_eq!(code, 2, "failed requests answer with response 2");
    assert!(results.is_empty());

    // A malformed option is rejected by the method call itself.
    let handle = "/org/freedesktop/portal/desktop/request/xdgtest/bad_token";
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("multiple", Value::from("yes"));
    assert!(call(&conn, "OpenFile", handle, "Open", options).is_err());

    // Cancel: the user closed the dialog without a choice. The reply is
    // response 1 with no results. A backend in cancel mode answers it; the
    // first backend is stopped before the second takes the bus name.
    drop(service);
    let _cancelling = start_service("--test-cancel");
    let handle = "/org/freedesktop/portal/desktop/request/xdgtest/cancel_token";
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("current_folder", Value::from(nul_terminated(folder.path())));
    let (code, results) = call(&conn, "OpenFile", handle, "Open", options).expect("OpenFile");
    assert_eq!(code, 1, "a cancelled request answers with response 1");
    assert!(results.is_empty(), "a cancelled request has no results");
}
