//! Persistent background application index worker with directory monitoring.
//!
//! Replaces synchronous startup scanning and per-search thread spawning with a
//! single background index worker that watches desktop directories and streams
//! versioned, deduplicated updates to GTK.

use crate::app_finder::{application_dirs, scan_desktop_apps, DesktopApp};
use gio::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AppIndexUpdate {
    pub seq: u64,
    pub apps: Arc<Vec<DesktopApp>>,
}

pub struct AppIndex {
    running: Arc<AtomicBool>,
    refresh_tx: Sender<()>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl AppIndex {
    pub fn start<F>(mut callback: F) -> Self
    where
        F: FnMut(AppIndexUpdate) + Send + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let (refresh_tx, refresh_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
        let seq_counter = Arc::new(AtomicU64::new(0));
        let seq_clone = seq_counter.clone();

        // Install GIO directory monitors on the GTK/GIO main thread if available
        let refresh_sender = refresh_tx.clone();
        for dir in application_dirs() {
            if dir.exists() {
                let file = gio::File::for_path(&dir);
                if let Ok(monitor) =
                    file.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
                {
                    let sender = refresh_sender.clone();
                    monitor.connect_changed(move |_, _, _, _| {
                        let _ = sender.send(());
                    });
                }
            }
        }

        let worker_handle = thread::Builder::new()
            .name("slopos-app-index".to_string())
            .spawn(move || {
                // Initial background scan
                let initial_apps = scan_desktop_apps();
                let initial_seq = seq_clone.fetch_add(1, Ordering::SeqCst) + 1;
                callback(AppIndexUpdate {
                    seq: initial_seq,
                    apps: Arc::new(initial_apps),
                });

                let mut last_scan = Instant::now();

                while running_clone.load(Ordering::Relaxed) {
                    // Wait for a refresh request or a debounce window
                    match refresh_rx.recv_timeout(Duration::from_millis(500)) {
                        Ok(()) => {
                            // Drain any queued refresh triggers to deduplicate
                            while refresh_rx.try_recv().is_ok() {}

                            // Debounce scans so rapid filesystem events trigger only one scan
                            if last_scan.elapsed() < Duration::from_millis(50) {
                                thread::sleep(Duration::from_millis(50) - last_scan.elapsed());
                            }

                            last_scan = Instant::now();
                            let apps = scan_desktop_apps();
                            let seq = seq_clone.fetch_add(1, Ordering::SeqCst) + 1;
                            callback(AppIndexUpdate {
                                seq,
                                apps: Arc::new(apps),
                            });
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Periodic background check if directory monitors miss events
                            if last_scan.elapsed() >= Duration::from_secs(30) {
                                last_scan = Instant::now();
                                let apps = scan_desktop_apps();
                                let seq = seq_clone.fetch_add(1, Ordering::SeqCst) + 1;
                                callback(AppIndexUpdate {
                                    seq,
                                    apps: Arc::new(apps),
                                });
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("failed to spawn application index worker");

        Self {
            running,
            refresh_tx,
            worker_handle: Some(worker_handle),
        }
    }

    pub fn request_refresh(&self) {
        let _ = self.refresh_tx.send(());
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.refresh_tx.send(());
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AppIndex {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_index_starts_and_shuts_down_cleanly() {
        let (tx, rx) = mpsc::channel();
        let mut index = AppIndex::start(move |update| {
            let _ = tx.send(update);
        });

        // Verify initial update arrives
        let initial = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("initial scan received");
        assert_eq!(initial.seq, 1);

        // Request refresh
        index.request_refresh();
        let second = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("refreshed update received");
        assert!(second.seq >= 2);

        // Verify clean shutdown
        index.stop();
    }
}
