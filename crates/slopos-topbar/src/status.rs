//! Live system state for the status items: one current-thread Tokio runtime
//! connects the `slopos-services` clients, forwards their snapshot streams
//! and runs the bar's changes. It blocks on the streams and the command
//! channel; every update wakes the bar. A missing provider is reported as
//! unavailable with the reason, never as a made-up value.

use std::sync::mpsc;
use std::thread;

use slopos_services::audio::{AudioClient, AudioDevice, AudioSnapshot};
use slopos_services::bluetooth::{BluetoothClient, BluetoothSnapshot};
use slopos_services::brightness::{BrightnessClient, BrightnessSnapshot};
use slopos_services::network::{NetworkDeviceType, NetworkManagerClient, NetworkSnapshot};
use slopos_services::power::{BatteryStatus, PowerClient, PowerEvent, PowerSnapshot};
use slopos_services::timedate::{TimedateClient, TimedateSnapshot};
use tokio::sync::mpsc as async_mpsc;

/// A provider's latest answer: a snapshot, or why there is none.
pub type State<T> = Result<T, String>;

/// Whether opening the mixer should ask the sound server for a connection. A
/// connection whose last answer was an error still exists while its watcher
/// retries, so a failed answer counts as lost even with a client in hand.
pub fn needs_audio_reconnect(connected: bool, last_answer_failed: bool) -> bool {
    !connected || last_answer_failed
}

/// Whether the sound server is unreachable after this answer. A snapshot that
/// follows a write is an answer too, so a good one clears an earlier failure.
pub fn audio_failed_after<T>(answer: &State<T>) -> bool {
    answer.is_err()
}

#[derive(Debug, Clone)]
pub enum Update {
    Audio(State<AudioSnapshot>),
    Network(State<NetworkSnapshot>),
    Power(State<PowerSnapshot>),
    /// The backlight, as the brightness service reports a change to it. The
    /// service's watch is the source of the brightness OSD.
    Brightness(State<BrightnessSnapshot>),
    /// Answer to `Command::RefreshBluetooth`.
    Bluetooth(State<BluetoothSnapshot>),
    /// Answer to `Command::RefreshTimedate`.
    Timedate(State<TimedateSnapshot>),
    /// A Volume Mixer change failed on the sound server.
    AudioFailed(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetOutputVolume(String, u32),
    SetOutputMuted(String, bool),
    SetDefaultOutput(String),
    /// A Volume Mixer change, run on the sound server.
    Mixer(slopos_mixer::Command),
    /// The Volume Mixer was opened: connect to the sound server again, if it was
    /// unavailable. Sent by the user's action, not on a timer.
    ReconnectAudio,
    /// The Bluetooth popover was opened: read the adapter and devices now. Sent by
    /// the user's action, not on a timer (the service's own watch polls).
    RefreshBluetooth,
    /// The clock popover was opened: read the time zone and network time now.
    RefreshTimedate,
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
    let mut audio = AudioClient::connect().await;
    let network = NetworkManagerClient::connect().await;
    let power = PowerClient::connect().await;
    // Read on demand: connected by the first request, and again after a failed one.
    let mut bluetooth_client: Option<BluetoothClient> = None;
    let mut timedate_client: Option<TimedateClient> = None;

    // Whether the last answer about the sound server was an error. A watcher that
    // reports one keeps its client while it retries, so `audio` alone cannot say so.
    let mut audio_failed = audio.is_err();
    let mut audio_events = None;
    match &audio {
        Ok(client) => {
            let state = client.snapshot().await.map_err(|e| e.to_string());
            audio_failed = state.is_err();
            send(Update::Audio(state));
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
    // The backlight is read now; its service's watch reports later changes.
    let brightness = BrightnessClient::new();
    send(Update::Brightness(
        brightness.snapshot().map_err(|e| e.to_string()),
    ));
    let mut brightness_events = Some(brightness.watch());

    loop {
        tokio::select! {
            Some(result) = next_or_pending!(audio_events) => {
                let state = result.map_err(|e| e.to_string());
                audio_failed = state.is_err();
                send(Update::Audio(state));
            }
            Some(result) = next_or_pending!(network_events) => send(Update::Network(result.map_err(|e| e.to_string()))),
            Some(result) = next_or_pending!(brightness_events) => send(Update::Brightness(result.map_err(|e| e.to_string()))),
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
                    Command::ReconnectAudio => {
                        if needs_audio_reconnect(audio.is_ok(), audio_failed) {
                            match AudioClient::connect().await {
                                Ok(client) => {
                                    let snapshot = client.snapshot().await.map_err(|e| e.to_string());
                                    match &snapshot {
                                        Ok(_) => log::info!("reconnected to the sound server"),
                                        Err(error) => log::warn!("reconnected to the sound server, but could not read it: {error}"),
                                    }
                                    audio_failed = audio_failed_after(&snapshot);
                                    audio_events = Some(client.watch());
                                    audio = Ok(client);
                                    send(Update::Audio(snapshot));
                                }
                                Err(error) => {
                                    log::warn!("could not reconnect to the sound server: {error}");
                                    audio_failed = true;
                                    send(Update::Audio(Err(error.to_string())));
                                }
                            }
                        }
                        Ok(None)
                    }
                    Command::Mixer(command) => {
                        match &audio {
                            Ok(client) => match slopos_mixer::apply(client, &command).await {
                                Ok(snapshot) => {
                                    audio_failed = false;
                                    send(Update::Audio(Ok(snapshot)));
                                }
                                Err(error) => {
                                    send(Update::AudioFailed(error.to_string()));
                                    // Show the server's state again, so the controls drop the value that
                                    // failed. If it cannot be read, the controls say so instead of
                                    // keeping the value that was asked for.
                                    let snapshot = client.snapshot().await.map_err(|e| e.to_string());
                                    audio_failed = audio_failed_after(&snapshot);
                                    send(Update::Audio(snapshot));
                                }
                            },
                            Err(error) => send(Update::AudioFailed(error.to_string())),
                        }
                        Ok(None)
                    }
                    Command::RefreshBluetooth => {
                        Ok(Some(Update::Bluetooth(read_bluetooth(&mut bluetooth_client).await)))
                    }
                    Command::RefreshTimedate => {
                        Ok(Some(Update::Timedate(read_timedate(&mut timedate_client).await)))
                    }
                    Command::Suspend => match &power {
                        Ok(client) => client.request_suspend().await.map(|()| None).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                };
                match outcome {
                    Ok(Some(update)) => {
                        // A snapshot from a write is an answer from the server, like any other.
                        if let Update::Audio(state) = &update {
                            audio_failed = audio_failed_after(state);
                        }
                        send(update);
                    }
                    Ok(None) => {}
                    Err(error) => send(Update::Failed(error)),
                }
            }
        }
    }
}

/// Bluetooth as it is now. A failed read drops the connection, so the next
/// request connects again.
async fn read_bluetooth(client: &mut Option<BluetoothClient>) -> State<BluetoothSnapshot> {
    if client.is_none() {
        *client = Some(
            BluetoothClient::connect()
                .await
                .map_err(|e| e.to_string())?,
        );
    }
    let answer = match client.as_ref() {
        Some(connected) => connected.snapshot().await.map_err(|e| e.to_string()),
        None => Err("Bluetooth is not connected".into()),
    };
    if answer.is_err() {
        *client = None;
    }
    answer
}

/// Time zone and network time as they are now; reconnects after a failure as
/// `read_bluetooth` does.
async fn read_timedate(client: &mut Option<TimedateClient>) -> State<TimedateSnapshot> {
    if client.is_none() {
        *client = Some(TimedateClient::connect().await.map_err(|e| e.to_string())?);
    }
    let answer = match client.as_ref() {
        Some(connected) => connected.snapshot().await.map_err(|e| e.to_string()),
        None => Err("timedate is not connected".into()),
    };
    if answer.is_err() {
        *client = None;
    }
    answer
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

/// The battery item: the charge, "Battery unavailable" when the power provider
/// cannot answer, or `None` when there is no battery to show or the first answer
/// has not come yet.
pub fn battery_item(state: Option<&State<PowerSnapshot>>) -> Option<String> {
    let state = state?;
    if state.is_err() {
        return Some("Battery unavailable".into());
    }
    battery_text(state)
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

    #[test]
    fn a_failed_answer_asks_for_a_reconnect_even_with_a_client_in_hand() {
        // Server lost after a good connection: the client is still held while its
        // watcher retries, so the mixer's opening must still ask for a connection.
        assert!(needs_audio_reconnect(true, true));
        assert!(needs_audio_reconnect(false, false));
        assert!(needs_audio_reconnect(false, true));
        // A live connection whose last answer was good needs nothing.
        assert!(!needs_audio_reconnect(true, false));
    }

    #[test]
    fn a_failed_snapshot_after_a_failed_command_keeps_the_server_marked_lost() {
        // The command's failure is reported on its own; the snapshot after it decides
        // whether the server is still lost, and an unreadable one means a reconnect.
        let snapshot: State<()> = Err("no PulseAudio server".into());
        let lost = audio_failed_after(&snapshot);
        assert!(lost);
        assert!(needs_audio_reconnect(true, lost));
    }

    #[test]
    fn the_battery_item_is_unavailable_without_a_power_provider_and_hidden_without_a_battery() {
        use slopos_services::power::{Battery, PowerSource, SuspendCapability};
        // Before the first answer there is no item to show.
        assert_eq!(battery_item(None), None);
        assert_eq!(
            battery_item(Some(&Err("no UPower".into()))),
            Some("Battery unavailable".into())
        );
        let snapshot = |battery| PowerSnapshot {
            revision: 1,
            availability: Availability::Available,
            battery,
            source: PowerSource::Unknown,
            suspend: SuspendCapability::Unknown,
            lid_present: false,
            lid_closed: None,
        };
        assert_eq!(battery_item(Some(&Ok(snapshot(None)))), None);
        let charge = Battery {
            percentage: 73.6,
            status: BatteryStatus::Discharging,
            warning: slopos_services::power::BatteryWarning::None,
            energy_rate_watts: None,
        };
        assert_eq!(
            battery_item(Some(&Ok(snapshot(Some(charge))))),
            Some("74%".into())
        );
    }

    #[test]
    fn a_good_snapshot_clears_an_earlier_failure() {
        let snapshot: State<()> = Ok(());
        let lost = audio_failed_after(&snapshot);
        assert!(!lost);
        assert!(!needs_audio_reconnect(true, lost));
    }
}
