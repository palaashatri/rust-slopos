//! slopos-portal: the SLOPOS file chooser backend. It serves
//! `org.freedesktop.impl.portal.FileChooser` as `org.freedesktop.impl.portal.desktop.slopos`
//! on the session bus, for xdg-desktop-portal to call.
//!
//! `--test-first-file` answers every request with the first matching file
//! instead of showing a dialog, and `--test-cancel` answers every request as
//! cancelled. They exist for the bus integration test and are not used by a
//! desktop session.

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use slopos_core::process::{first_available, FILE_CHOOSERS};
use slopos_portal::backend::{Backend, CancelBackend, FirstFileBackend, ProcessBackend};
use slopos_portal::service;

fn main() -> ExitCode {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let test_mode = std::env::args()
        .skip(1)
        .find(|arg| arg == "--test-first-file" || arg == "--test-cancel");
    let backend: Arc<dyn Backend> = match test_mode.as_deref() {
        Some("--test-first-file") => Arc::new(FirstFileBackend),
        Some(_) => Arc::new(CancelBackend),
        None => {
            if first_available(FILE_CHOOSERS).is_none() {
                eprintln!("slopos-portal: slopos-file-chooser is not installed beside this program or on PATH");
                return ExitCode::FAILURE;
            }
            Arc::new(ProcessBackend::default())
        }
    };
    // Held for the life of the process: dropping the connection unregisters the backend.
    let _connection = match service::serve(backend) {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!(
                "slopos-portal: could not register {}: {error}",
                service::BUS_NAME
            );
            return ExitCode::FAILURE;
        }
    };
    println!("slopos-portal: ready");
    let _ = std::io::stdout().flush();
    loop {
        std::thread::park();
    }
}
