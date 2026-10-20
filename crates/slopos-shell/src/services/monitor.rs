//! Persistent event-driven system and hardware service monitor.
//!
//! Consumes shared provider state off the GTK thread and publishes only changed shell status.

use super::{audio, network};
use slopos_services::power::{
    BatteryWarning, PowerClient, PowerEvent, PowerSnapshot, SuspendCapability,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    pub audio_text: String,
    pub network_text: String,
    pub battery_text: Option<String>,
    pub power_notification: Option<PowerNotification>,
    pub suspend_capability: Option<SuspendCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerNotification {
    pub critical: bool,
    pub percentage: u8,
}

impl SystemStatus {
    fn collect(
        audio_state: Option<audio::AudioState>,
        power_state: Option<PowerSnapshot>,
        power_notification: Option<PowerNotification>,
    ) -> Self {
        let audio_text = audio::audio_label_text(audio_state.as_ref());

        let network_state = network::query_network_status();
        let network_text = network::network_label_text(&network_state);

        let battery_text = power_state
            .as_ref()
            .and_then(|snapshot| snapshot.battery.as_ref())
            .map(|battery| {
                if battery.status == slopos_services::power::BatteryStatus::Charging {
                    format!("⚡{}%", battery.percentage.round() as u8)
                } else {
                    format!("{}%", battery.percentage.round() as u8)
                }
            });
        let suspend_capability = power_state.as_ref().map(|snapshot| snapshot.suspend);

        Self {
            audio_text,
            network_text,
            battery_text,
            power_notification,
            suspend_capability,
        }
    }
}

pub struct SystemMonitor {
    running: Arc<AtomicBool>,
    trigger_tx: Sender<()>,
    aggregator_handle: Option<thread::JoinHandle<()>>,
    audio_handle: Option<JoinHandle<()>>,
    power_shutdown: Option<tokio::sync::watch::Sender<bool>>,
    power_handle: Option<thread::JoinHandle<()>>,
}

impl SystemMonitor {
    pub fn start<F>(mut callback: F) -> Self
    where
        F: FnMut(SystemStatus) + Send + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let (trigger_tx, trigger_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
        let current_audio = Arc::new(Mutex::new(None));
        let current_power = Arc::new(Mutex::new(None));
        let current_power_notification = Arc::new(Mutex::new(None));
        let audio_for_worker = current_audio.clone();
        let trigger_audio = trigger_tx.clone();
        let audio_handle = audio::watch_audio_changes(move |state| {
            *audio_for_worker.lock().expect("audio state lock poisoned") = state;
            let _ = trigger_audio.send(());
        });

        let (power_shutdown, mut power_stopped) = tokio::sync::watch::channel(false);
        let power_for_worker = current_power.clone();
        let warning_for_worker = current_power_notification.clone();
        let trigger_power = trigger_tx.clone();
        let power_handle = thread::Builder::new()
            .name("slopos-power-provider".to_owned())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(_) => return,
                };
                runtime.block_on(async move {
                    'reconnect: loop {
                        if *power_stopped.borrow() {
                            break;
                        }
                        let client = match PowerClient::connect().await {
                            Ok(client) => client,
                            Err(_) => {
                                tokio::select! {
                                    _ = tokio::time::sleep(Duration::from_secs(2)) => continue,
                                    _ = power_stopped.changed() => break,
                                }
                            }
                        };
                        let mut events = client.watch();
                        loop {
                            tokio::select! {
                                _ = power_stopped.changed() => {
                                    events.close().await;
                                    break 'reconnect;
                                }
                                update = events.next() => match update {
                                    Some(Ok(PowerEvent::Changed(snapshot))) => {
                                        let warning = snapshot.battery.as_ref().map(|battery| battery.warning);
                                        if !matches!(warning, Some(BatteryWarning::Low | BatteryWarning::Critical)) {
                                            *warning_for_worker.lock().expect("power warning lock poisoned") = None;
                                        }
                                        *power_for_worker.lock().expect("power state lock poisoned") = Some(snapshot);
                                        let _ = trigger_power.send(());
                                    }
                                    Some(Ok(PowerEvent::LowBattery { percentage })) => {
                                        *warning_for_worker.lock().expect("power warning lock poisoned") = Some(PowerNotification {
                                            critical: false,
                                            percentage: percentage.round() as u8,
                                        });
                                        let _ = trigger_power.send(());
                                    }
                                    Some(Ok(PowerEvent::CriticalBattery { percentage })) => {
                                        *warning_for_worker.lock().expect("power warning lock poisoned") = Some(PowerNotification {
                                            critical: true,
                                            percentage: percentage.round() as u8,
                                        });
                                        let _ = trigger_power.send(());
                                    }
                                    Some(Err(_)) => {
                                        *power_for_worker.lock().expect("power state lock poisoned") = None;
                                        *warning_for_worker.lock().expect("power warning lock poisoned") = None;
                                        let _ = trigger_power.send(());
                                    }
                                    None => {
                                        events.close().await;
                                        continue 'reconnect;
                                    }
                                }
                            }
                        }
                    }
                });
            })
            .expect("spawn power provider worker");

        // 2. D-Bus system signal subscriber thread (UPower, NetworkManager, BlueZ)
        let running_dbus = running.clone();
        let trigger_dbus = trigger_tx.clone();
        let _ = thread::Builder::new()
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
            });

        // 3. Central status aggregator worker
        let running_agg = running.clone();
        let audio_for_aggregator = current_audio;
        let aggregator_handle = thread::Builder::new()
            .name("slopos-status-agg".to_string())
            .spawn(move || {
                let mut last_status = SystemStatus::collect(
                    audio_for_aggregator
                        .lock()
                        .expect("audio state lock poisoned")
                        .clone(),
                    current_power
                        .lock()
                        .expect("power state lock poisoned")
                        .clone(),
                    *current_power_notification
                        .lock()
                        .expect("power warning lock poisoned"),
                );
                callback(last_status.clone());

                let mut last_collect = Instant::now();

                while running_agg.load(Ordering::Relaxed) {
                    match trigger_rx.recv_timeout(Duration::from_secs(10)) {
                        Ok(()) => {
                            if !running_agg.load(Ordering::Relaxed) {
                                break;
                            }
                            // Drain queued triggers to debounce rapid bursts
                            while trigger_rx.try_recv().is_ok() {}

                            if last_collect.elapsed() >= Duration::from_millis(150) {
                                last_collect = Instant::now();
                                let new_status = SystemStatus::collect(
                                    audio_for_aggregator
                                        .lock()
                                        .expect("audio state lock poisoned")
                                        .clone(),
                                    current_power
                                        .lock()
                                        .expect("power state lock poisoned")
                                        .clone(),
                                    *current_power_notification
                                        .lock()
                                        .expect("power warning lock poisoned"),
                                );
                                if new_status != last_status {
                                    last_status = new_status.clone();
                                    callback(new_status);
                                }
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if !running_agg.load(Ordering::Relaxed) {
                                break;
                            }
                            // Infrequent sanity heartbeat in case kernel sysfs changed without D-Bus signals
                            let new_status = SystemStatus::collect(
                                audio_for_aggregator
                                    .lock()
                                    .expect("audio state lock poisoned")
                                    .clone(),
                                current_power
                                    .lock()
                                    .expect("power state lock poisoned")
                                    .clone(),
                                *current_power_notification
                                    .lock()
                                    .expect("power warning lock poisoned"),
                            );
                            if new_status != last_status {
                                last_status = new_status.clone();
                                callback(new_status);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn aggregator thread");

        Self {
            running,
            trigger_tx,
            aggregator_handle: Some(aggregator_handle),
            audio_handle: Some(audio_handle),
            power_shutdown: Some(power_shutdown),
            power_handle: Some(power_handle),
        }
    }

    pub fn trigger_refresh(&self) {
        let _ = self.trigger_tx.send(());
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.trigger_tx.send(());
        if let Some(handle) = self.audio_handle.take() {
            handle.abort();
        }
        if let Some(shutdown) = self.power_shutdown.take() {
            let _ = shutdown.send(true);
        }
        if let Some(handle) = self.power_handle.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.aggregator_handle.take() {
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

    #[test]
    fn unavailable_battery_state_does_not_create_a_battery_label() {
        let status = SystemStatus::collect(None, None, None);
        assert!(status.battery_text.is_none());
    }

    #[test]
    fn logind_suspend_capability_survives_missing_battery_provider() {
        let snapshot = PowerSnapshot {
            revision: 1,
            availability: slopos_core::capability::Availability::MissingService,
            battery: None,
            source: slopos_services::power::PowerSource::Unknown,
            suspend: SuspendCapability::RequiresAuthorization,
            lid_present: false,
            lid_closed: None,
        };
        let status = SystemStatus::collect(None, Some(snapshot), None);
        assert_eq!(
            status.suspend_capability,
            Some(SuspendCapability::RequiresAuthorization)
        );
    }
}
