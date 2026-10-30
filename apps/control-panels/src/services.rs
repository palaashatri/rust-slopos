//! The provider thread: one current-thread Tokio runtime that connects every
//! `slopos-services` client, forwards their snapshot streams to the window
//! and runs the window's changes. It blocks on the streams and the command
//! channel; nothing polls. Every update wakes the window.

use std::sync::mpsc;
use std::thread;

use slopos_services::audio::{AudioClient, AudioSnapshot};
use slopos_services::bluetooth::{BluetoothClient, BluetoothSnapshot};
use slopos_services::brightness::{BrightnessClient, BrightnessSnapshot};
use slopos_services::display::{DisplayClient, DisplaySnapshot, OutputConfig};
use slopos_services::input::{self, InputClient, InputSettings, InputSnapshot};
use slopos_services::network::{NetworkManagerClient, NetworkSnapshot};
use slopos_services::timedate::{TimedateClient, TimedateSnapshot};
use tokio::sync::mpsc as async_mpsc;

use crate::model::{State, Unavailable};

#[derive(Debug, Clone)]
pub enum Update {
    Network(State<NetworkSnapshot>),
    Audio(State<AudioSnapshot>),
    Brightness(State<BrightnessSnapshot>),
    Bluetooth(State<BluetoothSnapshot>),
    Time(State<TimedateSnapshot>),
    Displays(State<DisplaySnapshot>),
    Input(State<InputSnapshot>),
    /// A display change finished: `None` when the server took it.
    DisplaysApplied(Option<String>),
    /// Names of the saved Wi-Fi profiles (joined without a password).
    SavedWifi(Vec<String>),
    /// Pairing with this device ended: `None` when it paired.
    PairingDone(String, Option<String>),
    /// A change the user asked for failed.
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SetNetworking(bool),
    SetWifi(bool),
    ScanWifi(String),
    /// Join (device, access point, SSID, password for a new secured network).
    JoinWifi(String, String, Vec<u8>, Option<String>),
    /// Disconnect an active connection.
    Disconnect(String),
    /// Forget the saved Wi-Fi profile with this name.
    ForgetWifi(String),
    SetOutputVolume(String, u32),
    SetOutputMuted(String, bool),
    SetInputVolume(String, u32),
    SetInputMuted(String, bool),
    SetDefaultOutput(String),
    SetDefaultInput(String),
    /// Per-application sound: (stream, percent), (stream, muted),
    /// (stream, output device).
    SetStreamVolume(u32, u32),
    SetStreamMuted(u32, bool),
    MoveStream(u32, String),
    SetBrightness(String, u32),
    SetBluetoothPowered(String, bool),
    /// Search for devices on this adapter, or stop.
    SetBluetoothDiscovery(String, bool),
    /// Pair with, trust and connect to a device.
    PairBluetooth(String),
    CancelBluetoothPairing(String),
    ConnectBluetooth(String),
    DisconnectBluetooth(String),
    /// Forget (adapter, device).
    ForgetBluetooth(String, String),
    SetNtp(bool),
    ApplyDisplays(Vec<OutputConfig>),
    /// Apply keyboard and mouse settings, then save them here.
    ApplyInput(InputSettings, Option<std::path::PathBuf>),
}

pub struct Services {
    commands: async_mpsc::UnboundedSender<Command>,
    pub updates: mpsc::Receiver<Update>,
}

impl Services {
    /// Start the provider thread; `wake` runs after every update.
    pub fn start(wake: impl Fn() + Send + 'static) -> std::io::Result<Self> {
        let (commands, receiver) = async_mpsc::unbounded_channel();
        let (sender, updates) = mpsc::channel();
        thread::Builder::new()
            .name("control-panels-services".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => runtime.block_on(run(receiver, sender, wake)),
                    Err(error) => {
                        let _ = sender.send(Update::Failed(format!(
                            "can't start the service thread: {error}"
                        )));
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

macro_rules! unavailable {
    ($error:expr) => {
        Unavailable::new($error.availability(), $error.to_string())
    };
}

/// Await the next item of an optional stream, or never.
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

    let network = NetworkManagerClient::connect().await;
    let audio = AudioClient::connect().await;
    let bluetooth = BluetoothClient::connect().await;
    let timedate = TimedateClient::connect().await;
    let brightness = BrightnessClient::new();
    // Pairing waits for the device (up to a minute), so it runs beside the
    // loop, which stays free to cancel it.
    let (paired_sender, mut paired) = async_mpsc::unbounded_channel::<Update>();
    let input = InputClient::connect();
    send(Update::Input(match &input {
        Ok(client) => client.snapshot().map_err(|e| unavailable!(e)),
        Err(error) => Err(unavailable!(error)),
    }));
    // RandR events arrive on the watcher's own thread.
    let display = DisplayClient::connect();
    match &display {
        Ok(client) => {
            send(Update::Displays(
                client.snapshot().map_err(|e| unavailable!(e)),
            ));
            let sender = paired_sender.clone();
            if let Err(error) = DisplayClient::watch(move |result| {
                let _ = sender.send(Update::Displays(result.map_err(|e| unavailable!(e))));
            }) {
                send(Update::Displays(Err(unavailable!(error))));
            }
        }
        Err(error) => send(Update::Displays(Err(unavailable!(error)))),
    }

    let mut network_events = None;
    match &network {
        Ok(client) => {
            send(Update::Network(
                client.snapshot().await.map_err(|e| unavailable!(e)),
            ));
            send(Update::SavedWifi(saved_wifi(client).await));
            network_events = Some(client.watch());
        }
        Err(error) => send(Update::Network(Err(unavailable!(error)))),
    }
    let mut audio_events = None;
    match &audio {
        Ok(client) => {
            send(Update::Audio(
                client.snapshot().await.map_err(|e| unavailable!(e)),
            ));
            audio_events = Some(client.watch());
        }
        Err(error) => send(Update::Audio(Err(unavailable!(error)))),
    }
    let mut bluetooth_events = None;
    match &bluetooth {
        Ok(client) => {
            send(Update::Bluetooth(
                client.snapshot().await.map_err(|e| unavailable!(e)),
            ));
            bluetooth_events = Some(client.watch());
        }
        Err(error) => send(Update::Bluetooth(Err(unavailable!(error)))),
    }
    let mut timedate_events = None;
    match &timedate {
        Ok(client) => {
            send(Update::Time(
                client.snapshot().await.map_err(|e| unavailable!(e)),
            ));
            timedate_events = Some(client.watch());
        }
        Err(error) => send(Update::Time(Err(unavailable!(error)))),
    }
    send(Update::Brightness(
        brightness.snapshot().map_err(|e| unavailable!(e)),
    ));
    let mut brightness_events = Some(brightness.watch());

    loop {
        tokio::select! {
            Some(result) = next_or_pending!(network_events) => {
                send(Update::Network(result.map_err(|e| unavailable!(e))));
                if let Ok(client) = &network {
                    send(Update::SavedWifi(saved_wifi(client).await));
                }
            }
            Some(result) = next_or_pending!(audio_events) => send(Update::Audio(result.map_err(|e| unavailable!(e)))),
            Some(result) = next_or_pending!(bluetooth_events) => send(Update::Bluetooth(result.map_err(|e| unavailable!(e)))),
            Some(result) = next_or_pending!(timedate_events) => send(Update::Time(result.map_err(|e| unavailable!(e)))),
            Some(result) = next_or_pending!(brightness_events) => send(Update::Brightness(result.map_err(|e| unavailable!(e)))),
            Some(update) = paired.recv() => send(update),
            command = commands.recv() => {
                let Some(command) = command else { break };
                let outcome: Result<Option<Update>, String> = match command {
                    Command::SetNetworking(on) => match &network {
                        Ok(client) => client.set_networking_enabled(on).await.map(|s| Some(Update::Network(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetWifi(on) => match &network {
                        Ok(client) => client.set_wireless_enabled(on).await.map(|s| Some(Update::Network(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::ScanWifi(device) => match &network {
                        Ok(client) => client.request_wifi_scan(&device).await.map(|()| None).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::JoinWifi(device, access_point, ssid, password) => match &network {
                        Ok(client) => client.join_wifi(&device, &access_point, &ssid, password.as_deref()).await.map(|()| None).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::Disconnect(active) => match &network {
                        Ok(client) => client.deactivate_connection(&active).await.map(|s| Some(Update::Network(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::ForgetWifi(name) => match &network {
                        Ok(client) => match client.saved_connections().await {
                            Ok(profiles) => match profiles.into_iter().find(|p| p.connection_type == "802-11-wireless" && p.id == name) {
                                Some(profile) => match client.forget_connection(&profile.object_path).await {
                                    Ok(()) => Ok(Some(Update::SavedWifi(saved_wifi(client).await))),
                                    Err(e) => Err(e.to_string()),
                                },
                                None => Err(format!("no saved network is called {name}")),
                            },
                            Err(e) => Err(e.to_string()),
                        },
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetOutputVolume(device, percent) => match &audio {
                        Ok(client) => client.set_output_volume(&device, percent).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetOutputMuted(device, muted) => match &audio {
                        Ok(client) => client.set_output_muted(&device, muted).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetInputVolume(device, percent) => match &audio {
                        Ok(client) => client.set_input_volume(&device, percent).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetInputMuted(device, muted) => match &audio {
                        Ok(client) => client.set_input_muted(&device, muted).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetDefaultOutput(device) => match &audio {
                        Ok(client) => client.set_default_output(&device).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetDefaultInput(device) => match &audio {
                        Ok(client) => client.set_default_input(&device).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetStreamVolume(stream, percent) => match &audio {
                        Ok(client) => client.set_stream_volume(stream, percent).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetStreamMuted(stream, muted) => match &audio {
                        Ok(client) => client.set_stream_muted(stream, muted).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::MoveStream(stream, device) => match &audio {
                        Ok(client) => client.move_stream(stream, &device).await.map(|s| Some(Update::Audio(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetBrightness(device, value) => brightness
                        .set(&device, value)
                        .await
                        .map(|s| Some(Update::Brightness(Ok(s))))
                        .map_err(|e| e.to_string()),
                    Command::SetBluetoothPowered(adapter, on) => match &bluetooth {
                        Ok(client) => client.set_powered(&adapter, on).await.map(|s| Some(Update::Bluetooth(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetBluetoothDiscovery(adapter, on) => match &bluetooth {
                        Ok(client) => client.set_discovery(&adapter, on).await.map(|s| Some(Update::Bluetooth(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::PairBluetooth(device) => match &bluetooth {
                        Ok(client) => {
                            let client = client.clone();
                            let sender = paired_sender.clone();
                            tokio::spawn(async move {
                                let _ = sender.send(pair(&client, &device).await);
                            });
                            Ok(None)
                        }
                        Err(e) => Err(e.to_string()),
                    },
                    Command::CancelBluetoothPairing(device) => match &bluetooth {
                        Ok(client) => client.cancel_pairing(&device).await.map(|()| None).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::ConnectBluetooth(device) => match &bluetooth {
                        Ok(client) => client.connect_device(&device).await.map(|s| Some(Update::Bluetooth(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::DisconnectBluetooth(device) => match &bluetooth {
                        Ok(client) => client.disconnect_device(&device).await.map(|s| Some(Update::Bluetooth(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::ForgetBluetooth(adapter, device) => match &bluetooth {
                        Ok(client) => client.remove_device(&adapter, &device).await.map(|s| Some(Update::Bluetooth(Ok(s)))).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Command::ApplyDisplays(configs) => match &display {
                        Ok(client) => match client.apply(&configs) {
                            Ok(snapshot) => {
                                send(Update::Displays(Ok(snapshot)));
                                Ok(Some(Update::DisplaysApplied(None)))
                            }
                            Err(error) => Ok(Some(Update::DisplaysApplied(Some(error.to_string())))),
                        },
                        Err(e) => Ok(Some(Update::DisplaysApplied(Some(e.to_string())))),
                    },
                    Command::ApplyInput(settings, path) => match &input {
                        Ok(client) => match client.apply(&settings) {
                            Ok(snapshot) => {
                                let saved = path.map_or(Ok(()), |path| input::save(&path, &settings));
                                send(Update::Input(Ok(snapshot)));
                                saved.map(|()| None).map_err(|e| format!("Changed, but couldn't save it: {e}"))
                            }
                            Err(error) => {
                                send(Update::Input(client.snapshot().map_err(|e| unavailable!(e))));
                                Err(error.to_string())
                            }
                        },
                        Err(e) => Err(e.to_string()),
                    },
                    Command::SetNtp(on) => match &timedate {
                        Ok(client) => client.set_ntp(on).await.map(|s| Some(Update::Time(Ok(s)))).map_err(|e| e.to_string()),
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

/// Saved Wi-Fi profile names; empty when they cannot be read.
/// Pair, then trust (so it reconnects by itself) and connect. A device
/// that paired but would not connect is still reported as paired.
async fn pair(client: &BluetoothClient, device: &str) -> Update {
    if let Err(error) = client.pair_device(device).await {
        return Update::PairingDone(device.to_owned(), Some(error.to_string()));
    }
    let _ = client.set_device_trusted(device, true).await;
    let _ = client.connect_device(device).await;
    Update::PairingDone(device.to_owned(), None)
}

async fn saved_wifi(client: &NetworkManagerClient) -> Vec<String> {
    client
        .saved_connections()
        .await
        .map(|profiles| {
            profiles
                .into_iter()
                .filter(|p| p.connection_type == "802-11-wireless")
                .map(|p| p.id)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
impl Services {
    /// A service handle wired to test channels instead of a provider thread.
    pub fn for_tests(
        commands: async_mpsc::UnboundedSender<Command>,
        updates: mpsc::Receiver<Update>,
    ) -> Self {
        Self { commands, updates }
    }
}
