//! What the Volume Mixer shows, and how its controls map to sound-server
//! commands. Pure: the channels come from a `slopos-services` snapshot, so
//! everything here is tested without a sound server.

use slopos_services::audio::{AudioClient, AudioDevice, AudioError, AudioSnapshot, AudioStream};

/// Applications shown as channels. More than this are counted in
/// [`Channels::hidden_applications`] and named in the status line; the
/// per-application settings stay in Control Panels.
pub const MAX_APPLICATIONS: usize = 8;

/// What a channel controls, by the sound server's own identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The default output device, by name.
    Output(String),
    /// The default input device, by name.
    Input(String),
    /// One application's playback stream, by index.
    Stream(u32),
}

/// A change the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetVolume { target: Target, percent: u32 },
    SetMuted { target: Target, muted: bool },
}

/// One fader and its mute button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub target: Target,
    /// "Master", "Microphone" or the application's name.
    pub name: String,
    /// The device or media the channel is about.
    pub detail: String,
    /// Average channel volume, 0..=100 (or more if the server allows it).
    pub percent: u32,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Channels {
    pub channels: Vec<Channel>,
    /// Applications playing sound that did not fit.
    pub hidden_applications: usize,
    /// Whether there is a default output device at all.
    pub has_output: bool,
}

/// The channels for a snapshot: the master output, the default input (when
/// there is one), then one channel per application playing sound.
pub fn channels(snapshot: &AudioSnapshot) -> Channels {
    let mut out = Channels::default();
    if let Some(device) = default_device(&snapshot.outputs, snapshot.default_output.as_deref()) {
        out.has_output = true;
        out.channels.push(device_channel(
            device,
            Target::Output(device.name.clone()),
            "Master",
        ));
    }
    if let Some(device) = default_device(&snapshot.inputs, snapshot.default_input.as_deref()) {
        out.channels.push(device_channel(
            device,
            Target::Input(device.name.clone()),
            "Microphone",
        ));
    }
    for stream in snapshot.streams.iter().take(MAX_APPLICATIONS) {
        out.channels.push(stream_channel(stream, &snapshot.outputs));
    }
    out.hidden_applications = snapshot.streams.len().saturating_sub(MAX_APPLICATIONS);
    out
}

fn default_device<'a>(
    devices: &'a [AudioDevice],
    default: Option<&str>,
) -> Option<&'a AudioDevice> {
    devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| default.and_then(|name| devices.iter().find(|device| device.name == name)))
}

fn device_channel(device: &AudioDevice, target: Target, name: &str) -> Channel {
    Channel {
        target,
        name: name.to_owned(),
        detail: device
            .description
            .clone()
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| device.name.clone()),
        percent: percent_of(&device.volume_millipercent),
        muted: device.muted,
    }
}

fn stream_channel(stream: &AudioStream, outputs: &[AudioDevice]) -> Channel {
    let on = outputs
        .iter()
        .find(|device| device.index == stream.output)
        .map(|device| {
            device
                .description
                .clone()
                .unwrap_or_else(|| device.name.clone())
        });
    let detail = match (&stream.media, on) {
        (Some(media), _) if !media.is_empty() => media.clone(),
        (_, Some(output)) => format!("Playing on {output}"),
        _ => String::new(),
    };
    Channel {
        target: Target::Stream(stream.index),
        name: if stream.application.is_empty() {
            "Application".to_owned()
        } else {
            stream.application.clone()
        },
        detail,
        percent: percent_of(&stream.volume_millipercent),
        muted: stream.muted,
    }
}

/// The average of per-channel volumes, in whole percent.
pub fn percent_of(volumes_millipercent: &[u32]) -> u32 {
    if volumes_millipercent.is_empty() {
        return 0;
    }
    let count = volumes_millipercent.len() as u64;
    let sum: u64 = volumes_millipercent.iter().map(|&v| u64::from(v)).sum();
    ((sum + count * 500) / (count * 1000)) as u32
}

/// The line above the faders: the provider's truth, never a made-up value.
pub fn status_line(channels: &Channels) -> String {
    if !channels.has_output {
        return "No output device".to_owned();
    }
    let applications = channels
        .channels
        .iter()
        .filter(|channel| matches!(channel.target, Target::Stream(_)))
        .count();
    match (applications, channels.hidden_applications) {
        (0, _) => "No applications are playing sound".to_owned(),
        (shown, 0) => format!("{shown} playing"),
        (shown, hidden) => format!("{shown} playing, {hidden} more in Control Panels"),
    }
}

/// Run one command on the sound server and return the snapshot it leaves.
pub async fn apply(client: &AudioClient, command: &Command) -> Result<AudioSnapshot, AudioError> {
    match command {
        Command::SetVolume { target, percent } => match target {
            Target::Output(device) => client.set_output_volume(device, *percent).await,
            Target::Input(device) => client.set_input_volume(device, *percent).await,
            Target::Stream(index) => client.set_stream_volume(*index, *percent).await,
        },
        Command::SetMuted { target, muted } => match target {
            Target::Output(device) => client.set_output_muted(device, *muted).await,
            Target::Input(device) => client.set_input_muted(device, *muted).await,
            Target::Stream(index) => client.set_stream_muted(*index, *muted).await,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::capability::Availability;
    use slopos_services::audio::AudioDirection;

    fn device(
        index: u32,
        name: &str,
        description: &str,
        is_default: bool,
        volume: u32,
    ) -> AudioDevice {
        AudioDevice {
            index,
            name: name.into(),
            description: Some(description.into()),
            direction: AudioDirection::Output,
            muted: false,
            volume_millipercent: vec![volume * 1000, volume * 1000],
            is_default,
        }
    }

    fn stream(index: u32, application: &str, output: u32, volume: u32) -> AudioStream {
        AudioStream {
            index,
            application: application.into(),
            media: None,
            output,
            muted: false,
            volume_millipercent: vec![volume * 1000],
        }
    }

    fn snapshot(streams: Vec<AudioStream>) -> AudioSnapshot {
        AudioSnapshot {
            revision: 1,
            availability: Availability::Available,
            default_output: Some("speakers".into()),
            default_input: None,
            outputs: vec![device(1, "speakers", "Built-in speakers", true, 60)],
            inputs: Vec::new(),
            streams,
        }
    }

    #[test]
    fn the_master_comes_first_then_one_channel_per_application() {
        let channels = channels(&snapshot(vec![
            stream(7, "Music", 1, 40),
            stream(8, "", 1, 90),
        ]));
        assert!(channels.has_output);
        assert_eq!(channels.channels.len(), 3);
        let master = &channels.channels[0];
        assert_eq!(master.name, "Master");
        assert_eq!(master.detail, "Built-in speakers");
        assert_eq!(master.target, Target::Output("speakers".into()));
        assert_eq!(master.percent, 60);
        assert_eq!(channels.channels[1].name, "Music");
        assert_eq!(channels.channels[1].target, Target::Stream(7));
        assert_eq!(channels.channels[1].detail, "Playing on Built-in speakers");
        // A stream without a name is still a channel, with a plain label.
        assert_eq!(channels.channels[2].name, "Application");
    }

    #[test]
    fn media_titles_replace_the_output_detail_and_inputs_come_second() {
        let mut snapshot = snapshot(vec![AudioStream {
            media: Some("Track 3".into()),
            ..stream(7, "Player", 1, 50)
        }]);
        snapshot.default_input = Some("mic".into());
        snapshot.inputs = vec![AudioDevice {
            direction: AudioDirection::Input,
            ..device(2, "mic", "Headset microphone", false, 80)
        }];
        let channels = channels(&snapshot);
        assert_eq!(channels.channels[1].name, "Microphone");
        assert_eq!(channels.channels[1].target, Target::Input("mic".into()));
        assert_eq!(channels.channels[2].detail, "Track 3");
    }

    #[test]
    fn applications_beyond_the_limit_are_counted_not_dropped_silently() {
        let streams: Vec<AudioStream> = (0..MAX_APPLICATIONS as u32 + 3)
            .map(|i| stream(100 + i, &format!("App {i}"), 1, 10))
            .collect();
        let channels = channels(&snapshot(streams));
        assert_eq!(channels.channels.len(), 1 + MAX_APPLICATIONS);
        assert_eq!(channels.hidden_applications, 3);
        assert_eq!(
            status_line(&channels),
            "8 playing, 3 more in Control Panels"
        );
    }

    #[test]
    fn the_status_line_says_what_the_provider_reports() {
        let empty = AudioSnapshot {
            outputs: Vec::new(),
            default_output: None,
            ..snapshot(Vec::new())
        };
        assert_eq!(status_line(&channels(&empty)), "No output device");
        let quiet = snapshot(Vec::new());
        assert_eq!(
            status_line(&channels(&quiet)),
            "No applications are playing sound"
        );
        let busy = snapshot(vec![stream(7, "Music", 1, 40)]);
        assert_eq!(status_line(&channels(&busy)), "1 playing");
    }

    #[test]
    fn percentages_are_averages_of_the_channels_rounded_to_whole_percent() {
        assert_eq!(percent_of(&[]), 0);
        assert_eq!(percent_of(&[50_000, 150_000]), 100);
        assert_eq!(percent_of(&[33_400]), 33);
        assert_eq!(percent_of(&[33_600]), 34);
    }
}
