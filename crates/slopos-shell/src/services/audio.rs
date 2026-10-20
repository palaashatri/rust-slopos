//! Shared audio-service client for shell status and media controls.

use slopos_services::audio::{AudioClient, AudioEventStream, AudioSnapshot};
use std::sync::OnceLock;
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinHandle,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioState {
    pub volume_percent: u8,
    pub is_muted: bool,
}

static AUDIO_CLIENT: OnceLock<AudioClient> = OnceLock::new();
static AUDIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn client() -> AudioClient {
    AUDIO_CLIENT.get_or_init(AudioClient::new).clone()
}

fn runtime() -> &'static Runtime {
    AUDIO_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("create shared audio-service runtime")
    })
}

pub fn query_audio_state() -> Option<AudioState> {
    runtime()
        .block_on(client().snapshot())
        .ok()
        .as_ref()
        .and_then(audio_state_from_snapshot)
}

pub fn audio_state_from_snapshot(snapshot: &AudioSnapshot) -> Option<AudioState> {
    let name = snapshot.default_output.as_deref()?;
    let output = snapshot.outputs.iter().find(|device| device.name == name)?;
    let channels = &output.volume_millipercent;
    if channels.is_empty() {
        return None;
    }
    let volume =
        channels.iter().map(|value| u64::from(*value)).sum::<u64>() / channels.len() as u64;
    Some(AudioState {
        volume_percent: ((volume + 500) / 1000).min(150) as u8,
        is_muted: output.muted,
    })
}

pub fn audio_label_text(state: Option<&AudioState>) -> String {
    match state {
        Some(state) if state.is_muted => "Mute".to_owned(),
        Some(state) => format!("{}%", state.volume_percent),
        None => "—".to_owned(),
    }
}

pub fn watch_audio_changes<F>(mut callback: F) -> JoinHandle<()>
where
    F: FnMut(Option<AudioState>) + Send + 'static,
{
    let client = client();
    runtime().spawn(async move {
        let mut events: AudioEventStream = client.watch();
        while let Some(update) = events.next().await {
            callback(update.ok().as_ref().and_then(audio_state_from_snapshot));
        }
        events.close().await;
    })
}

pub fn adjust_default_output_volume(delta: i16) {
    runtime().spawn(async move {
        let client = client();
        if let Err(error) = client.adjust_default_output_volume(delta).await {
            log::warn!("Could not change output volume: {error}");
        }
    });
}

pub fn toggle_default_output_mute() {
    runtime().spawn(async move {
        let client = client();
        if let Err(error) = client.toggle_default_output_mute().await {
            log::warn!("Could not toggle output mute: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::capability::Availability;
    use slopos_services::audio::{AudioDevice, AudioDirection};

    #[test]
    fn status_uses_default_output_and_averages_channel_levels() {
        let snapshot = AudioSnapshot {
            revision: 2,
            availability: Availability::Available,
            default_output: Some("speaker".into()),
            default_input: None,
            outputs: vec![AudioDevice {
                index: 0,
                name: "speaker".into(),
                description: None,
                direction: AudioDirection::Output,
                muted: false,
                volume_millipercent: vec![49_500, 50_500],
                is_default: true,
            }],
            inputs: Vec::new(),
        };
        assert_eq!(
            audio_state_from_snapshot(&snapshot),
            Some(AudioState {
                volume_percent: 50,
                is_muted: false,
            })
        );
    }

    #[test]
    fn absent_default_output_is_not_replaced_with_a_fallback() {
        let snapshot = AudioSnapshot {
            revision: 1,
            availability: Availability::UnsupportedHardware,
            default_output: None,
            default_input: None,
            outputs: Vec::new(),
            inputs: Vec::new(),
        };
        assert_eq!(audio_state_from_snapshot(&snapshot), None);
        assert_eq!(audio_label_text(None), "—");
    }
}
