//! Background system and hardware service monitor.
//!
//! Offloads all sysfs, D-Bus, and audio queries to a dedicated background worker
//! thread so the GTK main thread is never blocked by slow I/O or service calls.

use super::{audio, network, power};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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
}

impl SystemMonitor {
    pub fn start<F>(interval: Duration, mut callback: F) -> Self
    where
        F: FnMut(SystemStatus) + Send + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        thread::Builder::new()
            .name("slopos-system-monitor".to_string())
            .spawn(move || {
                // Emit initial status
                let mut last_status = SystemStatus::collect();
                callback(last_status.clone());

                while running_clone.load(Ordering::Relaxed) {
                    thread::sleep(interval);
                    if !running_clone.load(Ordering::Relaxed) {
                        break;
                    }

                    let new_status = SystemStatus::collect();
                    if new_status != last_status {
                        last_status = new_status.clone();
                        callback(new_status);
                    }
                }
            })
            .expect("failed to spawn system monitor thread");

        Self { running }
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Drop for SystemMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}
