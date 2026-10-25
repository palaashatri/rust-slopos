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
use std::time::Duration;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    pub audio_text: String,
    pub network: network::NetworkSummary,
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
        network: Option<network::NetworkSummary>,
    ) -> Self {
        let audio_text = audio::audio_label_text(audio_state.as_ref());
        let network = network.unwrap_or_else(network::NetworkSummary::checking);

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
            network,
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

        // 2. NetworkManager status (event-driven, reconnecting).
        let current_network = Arc::new(Mutex::new(None));
        let network_for_worker = current_network.clone();
        let trigger_network = trigger_tx.clone();
        network::watch(move |summary| {
            *network_for_worker
                .lock()
                .expect("network state lock poisoned") = Some(summary);
            let _ = trigger_network.send(());
        });

        // 3. Status aggregator: publishes only real changes, never polls.
        let running_agg = running.clone();
        let audio_for_aggregator = current_audio;
        let aggregator_handle = thread::Builder::new()
            .name("slopos-status-agg".to_string())
            .spawn(move || {
                let collect = || {
                    SystemStatus::collect(
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
                        current_network
                            .lock()
                            .expect("network state lock poisoned")
                            .clone(),
                    )
                };
                let mut last_status = collect();
                callback(last_status.clone());

                while trigger_rx.recv().is_ok() {
                    if !running_agg.load(Ordering::Relaxed) {
                        break;
                    }
                    // Coalesce a burst of provider updates into one refresh.
                    thread::sleep(Duration::from_millis(50));
                    while trigger_rx.try_recv().is_ok() {}
                    let new_status = collect();
                    if new_status != last_status {
                        last_status = new_status.clone();
                        callback(new_status);
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
        assert!(!initial.network.label.is_empty());

        // Trigger an event refresh
        monitor.trigger_refresh();
        let _ = rx.recv_timeout(Duration::from_millis(500));

        // Clean shutdown
        monitor.stop();
    }

    #[test]
    fn unavailable_battery_state_does_not_create_a_battery_label() {
        let status = SystemStatus::collect(None, None, None, None);
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
        let status = SystemStatus::collect(None, Some(snapshot), None, None);
        assert_eq!(
            status.suspend_capability,
            Some(SuspendCapability::RequiresAuthorization)
        );
    }
}
