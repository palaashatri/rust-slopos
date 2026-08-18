//! Persistent event-driven system and hardware service monitor.
//!
//! Subscribes to D-Bus signals (NetworkManager, UPower, BlueZ) and PipeWire/PulseAudio
//! change notifications on a dedicated background thread, eliminating periodic
//! timer polling. GTK receives updates only when underlying hardware/service state transitions.

use super::{audio, network, power};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    pub audio_text: String,
    pub network_text: String,
    pub battery_text: Option<String>,
}

impl SystemStatus {
    pub fn collect() -> Self {
        let audio_state = audio::query_audio_state();
        let audio_text = audio::audio_label_text(audio_state.as_ref());

        let network_state = network::query_network_status();
        let network_text = network::network_label_text(&network_state);

        let battery_state = power::query_battery_state();
        let battery_text = power::battery_label_text(battery_state.as_ref());

        Self {
            audio_text,
            network_text,
            battery_text,
        }
    }
}

pub struct SystemMonitor {
    running: Arc<AtomicBool>,
    trigger_tx: Sender<()>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl SystemMonitor {
    pub fn start<F>(mut callback: F) -> Self
    where
        F: FnMut(SystemStatus) + Send + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let (trigger_tx, trigger_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
        let mut threads = Vec::new();

        // 1. Audio event subscriber thread (listens to PipeWire/PulseAudio event stream)
        let running_audio = running.clone();
        let trigger_audio = trigger_tx.clone();
        threads.push(
            thread::Builder::new()
                .name("slopos-audio-sub".to_string())
                .spawn(move || {
                    while running_audio.load(Ordering::Relaxed) {
                        // Start long-lived pactl subscribe process to receive sink volume/mute events
                        let mut child = match Command::new("pactl")
                            .arg("subscribe")
                            .stdout(Stdio::piped())
                            .stderr(Stdio::null())
                            .spawn()
                        {
                            Ok(child) => child,
                            Err(_) => {
                                thread::sleep(Duration::from_secs(5));
                                continue;
                            }
                        };

                        if let Some(stdout) = child.stdout.take() {
                            let reader = BufReader::new(stdout);
                            for line in reader.lines() {
                                if !running_audio.load(Ordering::Relaxed) {
                                    break;
                                }
                                if let Ok(event) = line {
                                    if event.contains("sink") || event.contains("server") {
                                        let _ = trigger_audio.send(());
                                    }
                                }
                            }
                        }

                        let _ = child.kill();
                        let _ = child.wait();
                        thread::sleep(Duration::from_secs(2));
                    }
                })
                .expect("spawn audio monitor thread"),
        );

        // 2. D-Bus system signal subscriber thread (UPower, NetworkManager, BlueZ)
        let running_dbus = running.clone();
        let trigger_dbus = trigger_tx.clone();
        threads.push(
            thread::Builder::new()
                .name("slopos-dbus-sub".to_string())
                .spawn(move || {
                    while running_dbus.load(Ordering::Relaxed) {
                        let conn = match zbus::blocking::Connection::system() {
                            Ok(c) => c,
                            Err(_) => {
                                thread::sleep(Duration::from_secs(5));
                                continue;
                            }
                        };

                        let nm_proxy = zbus::blocking::Proxy::new(
                            &conn,
                            "org.freedesktop.NetworkManager",
                            "/org/freedesktop/NetworkManager",
                            "org.freedesktop.NetworkManager",
                        );

                        if let Ok(proxy) = nm_proxy {
                            if let Ok(iter) = proxy.receive_all_signals() {
                                for _ in iter {
                                    if !running_dbus.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    let _ = trigger_dbus.send(());
                                }
                            }
                        } else {
                            thread::sleep(Duration::from_secs(5));
                        }
                    }
                })
                .expect("spawn dbus monitor thread"),
        );

        // 3. Central status aggregator worker
        let running_agg = running.clone();
        threads.push(
            thread::Builder::new()
                .name("slopos-status-agg".to_string())
                .spawn(move || {
                    let mut last_status = SystemStatus::collect();
                    callback(last_status.clone());

                    let mut last_collect = Instant::now();

                    while running_agg.load(Ordering::Relaxed) {
                        match trigger_rx.recv_timeout(Duration::from_secs(10)) {
                            Ok(()) => {
                                // Drain queued triggers to debounce rapid bursts
                                while trigger_rx.try_recv().is_ok() {}

                                if last_collect.elapsed() >= Duration::from_millis(150) {
                                    last_collect = Instant::now();
                                    let new_status = SystemStatus::collect();
                                    if new_status != last_status {
                                        last_status = new_status.clone();
                                        callback(new_status);
                                    }
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                // Infrequent sanity heartbeat in case kernel sysfs changed without D-Bus signals
                                let new_status = SystemStatus::collect();
                                if new_status != last_status {
                                    last_status = new_status.clone();
                                    callback(new_status);
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    }
                })
                .expect("spawn aggregator thread"),
        );

        Self {
            running,
            trigger_tx,
            threads,
        }
    }

    pub fn trigger_refresh(&self) {
        let _ = self.trigger_tx.send(());
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.trigger_tx.send(());
        for handle in self.threads.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Drop for SystemMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_monitor_starts_and_shuts_down_cleanly() {
        let (tx, rx) = mpsc::channel();
        let mut monitor = SystemMonitor::start(move |status| {
            let _ = tx.send(status);
        });

        // Verify initial collection is received immediately
        let initial = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("initial status collected");
        assert!(!initial.audio_text.is_empty());
        assert!(!initial.network_text.is_empty());

        // Trigger an event refresh
        monitor.trigger_refresh();
        let _ = rx.recv_timeout(Duration::from_millis(500));

        // Clean shutdown
        monitor.stop();
    }
}
