//! Live system state for the status items: one current-thread Tokio runtime
//! connects the `slopos-services` clients, forwards their snapshot streams
//! and runs the bar's changes. It blocks on the streams and the command
//! channel; every update wakes the bar. A missing provider is reported as
//! unavailable with the reason, never as a made-up value.

use std::sync::mpsc;
use std::thread;

use slopos_services::audio::{AudioClient, AudioDevice, AudioSnapshot};
use slopos_services::network::{NetworkDeviceType, NetworkManagerClient, NetworkSnapshot};
use slopos_services::power::{BatteryStatus, PowerClient, PowerEvent, PowerSnapshot};
use tokio::sync::mpsc as async_mpsc;

/// A provider's latest answer: a snapshot, or why there is none.
pub type State<T> = Result<T, String>;

#[derive(Debug, Clone)]
pub enum Update {
    Audio(State<AudioSnapshot>),
    Network(State<NetworkSnapshot>),
    Power(State<PowerSnapshot>),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetOutputVolume(String, u32),
    SetOutputMuted(String, bool),
    SetDefaultOutput(String),
    Suspend,
}

pub struct Status {
    commands: async_mpsc::UnboundedSender<Command>,
    pub updates: mpsc::Receiver<Update>,
}

impl Status {
    pub fn start(wake: impl Fn() + Send + 'static) -> std::io::Result<Self> {
        let (commands, receiver) = async_mpsc::unbounded_channel();
        let (sender, updates) = mpsc::channel();
        thread::Builder::new()
            .name("topbar-status".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(run(receiver, sender, wake)),
                    Err(error) => {
                        let _ = sender.send(Update::Failed(format!("status thread: {error}")));
                        wake();
                    }
                }
            })?;
        Ok(Self { commands, updates })
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

macro_rules! next_or_pending {
    ($stream:expr) => {
        async {
            match $stream.as_mut() {
                Some(stream) => stream.next().await,
                None => std::future::pending().await,
            }
        }
    };
}

async fn run(
    mut commands: async_mpsc::UnboundedReceiver<Command>,
    sender: mpsc::Sender<Update>,
    wake: impl Fn() + Send + 'static,
) {
    let send = |update: Update| {
        let _ = sender.send(update);
        wake();
    };
    let audio = AudioClient::connect().await;
    let network = NetworkManagerClient::connect().await;
    let power = PowerClient::connect().await;

    let mut audio_events = None;
    match &audio {
        Ok(client) => {
            send(Update::Audio(
                client.snapshot().await.map_err(|e| e.to_string()),
            ));
            audio_events = Some(client.watch());
        }
        Err(error) => send(Update::Audio(Err(error.to_string()))),
    }
    let mut network_events = None;
    match &network {
        Ok(client) => {
            send(Update::Network(
                client.snapshot().await.map_err(|e| e.to_string()),
            ));
            network_events = Some(client.watch());
        }
        Err(error) => send(Update::Network(Err(error.to_string()))),
    }
    let mut power_events = None;
    match &power {
        Ok(client) => {
            send(Update::Power(
                client.snapshot().await.map_err(|e| e.to_string()),
            ));
            power_events = Some(client.watch());
        }
        Err(error) => send(Update::Power(Err(error.to_string()))),
    }

    loop {
        tokio::select! {
            Some(result) = next_or_pending!(audio_events) => send(Update::Audio(result.map_err(|e| e.to_string()))),
            Some(result) = next_or_pending!(network_events) => send(Update::Network(result.map_err(|e| e.to_string()))),
            Some(result) = next_or_pending!(power_events) => match result {
                Ok(PowerEvent::Changed(snapshot)) => send(Update::Power(Ok(snapshot))),
                Ok(_) => {}
                Err(error) => send(Update::Power(Err(error.to_string()))),
            },
            command = commands.recv() => {
                let Some(command) = command else { break };
                let outcome = match command {
                    Command::SetOutputVolume(device, percent) => match &audio {
                        Ok(client) => client.set_output_volume(&device, percent).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetOutputMuted(device, muted) => match &audio {
                        Ok(client) => client.set_output_muted(&device, muted).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetDefaultOutput(device) => match &audio {
                        Ok(client) => client.set_default_output(&device).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::Suspend => match &power {
                        Ok(client) => client.request_suspend().await.map(|()| None).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                };
                match outcome {
                    Ok(Some(update)) => send(update),
                    Ok(None) => {}
                    Err(error) => send(Update::Failed(error)),
                }
            }
        }
    }
}

/// The default output device, if the sound server reported one.
pub fn default_output(snapshot: &AudioSnapshot) -> Option<&AudioDevice> {
    snapshot
        .outputs
        .iter()
        .find(|device| Some(&device.name) == snapshot.default_output.as_ref())
        .or_else(|| snapshot.outputs.iter().find(|device| device.is_default))
}

/// Average channel volume in percent.
pub fn volume_percent(device: &AudioDevice) -> u32 {
    if device.volume_millipercent.is_empty() {
        return 0;
    }
    let sum: u64 = device
        .volume_millipercent
        .iter()
        .map(|v| u64::from(*v))
        .sum();
    (sum / device.volume_millipercent.len() as u64 / 1000) as u32
}

/// Text next to the speaker: "45%", "Muted", "—" when there is no output.
pub fn sound_text(state: &State<AudioSnapshot>) -> String {
    match state.as_ref().ok().and_then(default_output) {
        Some(device) if device.muted => "Muted".into(),
        Some(device) => format!("{}%", volume_percent(device)),
        None => "—".into(),
    }
}

/// What the network item says: the connection name, "Offline", or why it
/// cannot tell.
pub fn network_text(state: &State<NetworkSnapshot>) -> String {
    match state {
        Err(_) => "Network unavailable".into(),
        Ok(snapshot) if !snapshot.networking_enabled => "Networking off".into(),
        Ok(snapshot) => snapshot
            .active_connections
            .iter()
            .find(|connection| !connection.is_vpn && connection.state == 2)
            .map(|connection| connection.id.clone())
            .unwrap_or_else(|| "Offline".into()),
    }
}

/// Whether the active connection is Wi-Fi (for the icon).
pub fn on_wifi(state: &State<NetworkSnapshot>) -> bool {
    state.as_ref().is_ok_and(|snapshot| {
        snapshot.devices.iter().any(|device| {
            device.kind == NetworkDeviceType::Wifi && device.active_connection.is_some()
        })
    })
}

/// Battery text, or `None` when there is no battery to show.
pub fn battery_text(state: &State<PowerSnapshot>) -> Option<String> {
    let battery = state.as_ref().ok()?.battery.as_ref()?;
    let charging = matches!(
        battery.status,
        BatteryStatus::Charging | BatteryStatus::Full | BatteryStatus::PendingCharge
    );
    Some(format!(
        "{:.0}%{}",
        battery.percentage.clamp(0.0, 100.0),
        if charging { " ⚡" } else { "" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::capability::Availability;
    use slopos_services::audio::AudioDirection;

    fn device(name: &str, volume: u32, muted: bool) -> AudioDevice {
        AudioDevice {
            index: 0,
            name: name.into(),
            description: Some("Speakers".into()),
            direction: AudioDirection::Output,
            muted,
            volume_millipercent: vec![volume * 1000, volume * 1000],
            is_default: false,
        }
    }

    #[test]
    fn status_text_comes_only_from_provider_state() {
        let snapshot = AudioSnapshot {
            revision: 1,
            availability: Availability::Available,
            default_output: Some("b".into()),
            default_input: None,
            outputs: vec![device("a", 10, false), device("b", 45, false)],
            inputs: Vec::new(),
            streams: Vec::new(),
        };
        assert_eq!(sound_text(&Ok(snapshot.clone())), "45%");
        let mut muted = snapshot;
        muted.outputs[1].muted = true;
        assert_eq!(sound_text(&Ok(muted)), "Muted");
        assert_eq!(sound_text(&Err("no server".into())), "—");
        assert_eq!(network_text(&Err("no NM".into())), "Network unavailable");
        let offline = NetworkSnapshot {
            revision: 1,
            networking_enabled: true,
            wireless_enabled: true,
            wireless_hardware_enabled: true,
            devices: Vec::new(),
            active_connections: Vec::new(),
        };
        assert_eq!(network_text(&Ok(offline.clone())), "Offline");
        assert!(!on_wifi(&Ok(offline)));
        assert_eq!(battery_text(&Err("no upower".into())), None);
    }
}
