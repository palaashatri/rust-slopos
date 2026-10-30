use std::{
    io,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use slopos_core::capability::Availability;
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, Mutex},
    task::JoinHandle,
    time::timeout,
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_COMMAND_OUTPUT: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDirection {
    Output,
    Input,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub index: u32,
    pub name: String,
    pub description: Option<String>,
    pub direction: AudioDirection,
    pub muted: bool,
    pub volume_millipercent: Vec<u32>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioStream {
    /// The sound server's index for the stream.
    pub index: u32,
    /// The application's name, as it reports it.
    pub application: String,
    /// What it is playing, when it says.
    pub media: Option<String>,
    /// Index of the output device it plays on.
    pub output: u32,
    pub muted: bool,
    pub volume_millipercent: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSnapshot {
    pub revision: u64,
    pub availability: Availability,
    pub default_output: Option<String>,
    pub default_input: Option<String>,
    pub outputs: Vec<AudioDevice>,
    pub inputs: Vec<AudioDevice>,
    /// Applications playing sound now.
    pub streams: Vec<AudioStream>,
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("PulseAudio-compatible service is unavailable: {0}")]
    MissingService(String),
    #[error("Audio service disconnected")]
    Disconnected,
    #[error("Audio operation was denied: {0}")]
    PermissionDenied(String),
    #[error("Audio command timed out")]
    Timeout,
    #[error("Audio device is no longer available: {0}")]
    DeviceUnavailable(String),
    #[error("Audio volume must be between 0 and 150 percent")]
    InvalidVolume,
    #[error("Invalid Pactl output: {0}")]
    InvalidOutput(String),
    #[error("Pactl command failed: {0}")]
    CommandFailed(String),
}

impl AudioError {
    pub fn availability(&self) -> Availability {
        match self {
            Self::MissingService(_) => Availability::MissingService,
            Self::Disconnected => Availability::Disconnected,
            Self::PermissionDenied(_) => Availability::PermissionDenied,
            Self::Timeout
            | Self::DeviceUnavailable(_)
            | Self::InvalidVolume
            | Self::InvalidOutput(_)
            | Self::CommandFailed(_) => Availability::Failed,
        }
    }
}

#[derive(Clone)]
pub struct AudioClient {
    executable: PathBuf,
    revision: Arc<AtomicU64>,
    mutation_lock: Arc<Mutex<()>>,
}

pub struct AudioEventStream {
    receiver: mpsc::Receiver<Result<AudioSnapshot, AudioError>>,
    task: Option<JoinHandle<()>>,
}

impl AudioEventStream {
    pub async fn next(&mut self) -> Option<Result<AudioSnapshot, AudioError>> {
        self.receiver.recv().await
    }

    pub async fn close(mut self) {
        self.receiver.close();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for AudioEventStream {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl AudioClient {
    pub fn new() -> Self {
        Self {
            executable: PathBuf::from("pactl"),
            revision: Arc::new(AtomicU64::new(0)),
            mutation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn connect() -> Result<Self, AudioError> {
        let client = Self::new();
        client.run(&["info"]).await?;
        Ok(client)
    }

    pub async fn snapshot(&self) -> Result<AudioSnapshot, AudioError> {
        let result = timeout(MUTATION_TIMEOUT, async {
            let (sinks, sources, info, streams) = tokio::try_join!(
                self.run(&["list", "sinks"]),
                self.run(&["list", "sources"]),
                self.run(&["info"]),
                self.run(&["list", "sink-inputs"]),
            )?;
            let streams = parse_streams(&streams)?;
            let (default_output, default_input) = parse_defaults(&info)?;
            let mut outputs = parse_devices(&sinks, "Sink", AudioDirection::Output)?;
            let mut inputs = parse_devices(&sources, "Source", AudioDirection::Input)?;
            inputs.retain(|device| !device.name.ends_with(".monitor"));
            for device in &mut outputs {
                device.is_default = default_output.as_deref() == Some(&device.name);
            }
            for device in &mut inputs {
                device.is_default = default_input.as_deref() == Some(&device.name);
            }
            Ok((default_output, default_input, outputs, inputs, streams))
        })
        .await
        .map_err(|_| AudioError::Timeout)??;
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        let availability = if result.2.is_empty() && result.3.is_empty() {
            Availability::UnsupportedHardware
        } else {
            Availability::Available
        };
        Ok(AudioSnapshot {
            revision,
            availability,
            default_output: result.0,
            default_input: result.1,
            outputs: result.2,
            inputs: result.3,
            streams: result.4,
        })
    }

    pub fn watch(&self) -> AudioEventStream {
        let client = self.clone();
        let (sender, receiver) = mpsc::channel(16);
        let task = tokio::spawn(async move { client.watch_task(sender).await });
        AudioEventStream {
            receiver,
            task: Some(task),
        }
    }

    pub async fn set_output_volume(
        &self,
        device: &str,
        percent: u32,
    ) -> Result<AudioSnapshot, AudioError> {
        self.set_volume(AudioDirection::Output, device, percent)
            .await
    }

    pub async fn adjust_default_output_volume(
        &self,
        delta_percent: i16,
    ) -> Result<AudioSnapshot, AudioError> {
        let _guard = self.mutation_lock.lock().await;
        let snapshot = self.snapshot().await?;
        let device = snapshot
            .default_output
            .as_deref()
            .ok_or_else(|| AudioError::DeviceUnavailable("default output".to_owned()))?;
        let output = snapshot
            .outputs
            .iter()
            .find(|output| output.name == device)
            .ok_or_else(|| AudioError::DeviceUnavailable(device.to_owned()))?;
        if output.volume_millipercent.is_empty() {
            return Err(AudioError::InvalidOutput(
                "Default output has no channel volume".to_owned(),
            ));
        }
        let average = output
            .volume_millipercent
            .iter()
            .map(|volume| u64::from(*volume))
            .sum::<u64>()
            / output.volume_millipercent.len() as u64;
        let current_percent = ((average + 500) / 1000).min(150) as i16;
        let target_percent =
            (i32::from(current_percent) + i32::from(delta_percent)).clamp(0, 150) as u32;
        self.set_volume_locked(AudioDirection::Output, device, target_percent)
            .await
    }

    pub async fn set_input_volume(
        &self,
        device: &str,
        percent: u32,
    ) -> Result<AudioSnapshot, AudioError> {
        self.set_volume(AudioDirection::Input, device, percent)
            .await
    }

    pub async fn set_output_muted(
        &self,
        device: &str,
        muted: bool,
    ) -> Result<AudioSnapshot, AudioError> {
        self.set_mute(AudioDirection::Output, device, muted).await
    }

    pub async fn toggle_default_output_mute(&self) -> Result<AudioSnapshot, AudioError> {
        let _guard = self.mutation_lock.lock().await;
        let snapshot = self.snapshot().await?;
        let device = snapshot
            .default_output
            .as_deref()
            .ok_or_else(|| AudioError::DeviceUnavailable("default output".to_owned()))?;
        let output = snapshot
            .outputs
            .iter()
            .find(|output| output.name == device)
            .ok_or_else(|| AudioError::DeviceUnavailable(device.to_owned()))?;
        self.set_mute_locked(AudioDirection::Output, device, !output.muted)
            .await
    }

    pub async fn set_input_muted(
        &self,
        device: &str,
        muted: bool,
    ) -> Result<AudioSnapshot, AudioError> {
        self.set_mute(AudioDirection::Input, device, muted).await
    }

    pub async fn set_default_output(&self, device: &str) -> Result<AudioSnapshot, AudioError> {
        self.set_default(AudioDirection::Output, device).await
    }

    pub async fn set_default_input(&self, device: &str) -> Result<AudioSnapshot, AudioError> {
        self.set_default(AudioDirection::Input, device).await
    }

    /// Set one application's volume.
    pub async fn set_stream_volume(
        &self,
        stream: u32,
        percent: u32,
    ) -> Result<AudioSnapshot, AudioError> {
        if percent > 150 {
            return Err(AudioError::InvalidVolume);
        }
        let _guard = self.mutation_lock.lock().await;
        self.ensure_stream(stream).await?;
        self.run(&[
            "set-sink-input-volume",
            &stream.to_string(),
            &format!("{percent}%"),
        ])
        .await?;
        self.wait_for_stream(stream, |item| {
            item.volume_millipercent
                .iter()
                .all(|volume| volume.abs_diff(percent * 1000) <= 1000)
        })
        .await
    }

    pub async fn set_stream_muted(
        &self,
        stream: u32,
        muted: bool,
    ) -> Result<AudioSnapshot, AudioError> {
        let _guard = self.mutation_lock.lock().await;
        self.ensure_stream(stream).await?;
        self.run(&[
            "set-sink-input-mute",
            &stream.to_string(),
            if muted { "1" } else { "0" },
        ])
        .await?;
        self.wait_for_stream(stream, |item| item.muted == muted)
            .await
    }

    /// Play an application's sound on another output device.
    pub async fn move_stream(
        &self,
        stream: u32,
        device: &str,
    ) -> Result<AudioSnapshot, AudioError> {
        let _guard = self.mutation_lock.lock().await;
        self.ensure_stream(stream).await?;
        let snapshot = self.snapshot().await?;
        let target = snapshot
            .outputs
            .iter()
            .find(|output| output.name == device)
            .map(|output| output.index)
            .ok_or_else(|| AudioError::DeviceUnavailable(device.to_owned()))?;
        self.run(&["move-sink-input", &stream.to_string(), device])
            .await?;
        self.wait_for_stream(stream, |item| item.output == target)
            .await
    }

    async fn ensure_stream(&self, stream: u32) -> Result<(), AudioError> {
        let snapshot = self.snapshot().await?;
        if snapshot.streams.iter().any(|item| item.index == stream) {
            Ok(())
        } else {
            Err(AudioError::DeviceUnavailable(format!("stream {stream}")))
        }
    }

    async fn wait_for_stream(
        &self,
        stream: u32,
        matches: impl Fn(&AudioStream) -> bool,
    ) -> Result<AudioSnapshot, AudioError> {
        timeout(MUTATION_TIMEOUT, async {
            loop {
                let snapshot = self.snapshot().await?;
                match snapshot.streams.iter().find(|item| item.index == stream) {
                    None => return Err(AudioError::DeviceUnavailable(format!("stream {stream}"))),
                    Some(item) if matches(item) => return Ok(snapshot),
                    Some(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        })
        .await
        .map_err(|_| AudioError::Timeout)?
    }

    async fn set_volume(
        &self,
        direction: AudioDirection,
        device: &str,
        percent: u32,
    ) -> Result<AudioSnapshot, AudioError> {
        if percent > 150 {
            return Err(AudioError::InvalidVolume);
        }
        // ponytail: serialize all audio writes; per-device locks if contention matters.
        let _guard = self.mutation_lock.lock().await;
        self.set_volume_locked(direction, device, percent).await
    }

    async fn set_volume_locked(
        &self,
        direction: AudioDirection,
        device: &str,
        percent: u32,
    ) -> Result<AudioSnapshot, AudioError> {
        self.ensure_device(direction, device).await?;
        let verb = match direction {
            AudioDirection::Output => "set-sink-volume",
            AudioDirection::Input => "set-source-volume",
        };
        self.run(&[verb, device, &format!("{percent}%")]).await?;
        self.wait_for(direction, device, |snapshot| {
            let devices = match direction {
                AudioDirection::Output => &snapshot.outputs,
                AudioDirection::Input => &snapshot.inputs,
            };
            devices
                .iter()
                .find(|item| item.name == device)
                .is_some_and(|item| {
                    item.volume_millipercent
                        .iter()
                        .all(|volume| volume.abs_diff(percent * 1000) <= 1000)
                })
        })
        .await
    }

    async fn set_mute(
        &self,
        direction: AudioDirection,
        device: &str,
        muted: bool,
    ) -> Result<AudioSnapshot, AudioError> {
        let _guard = self.mutation_lock.lock().await;
        self.set_mute_locked(direction, device, muted).await
    }

    async fn set_mute_locked(
        &self,
        direction: AudioDirection,
        device: &str,
        muted: bool,
    ) -> Result<AudioSnapshot, AudioError> {
        self.ensure_device(direction, device).await?;
        let verb = match direction {
            AudioDirection::Output => "set-sink-mute",
            AudioDirection::Input => "set-source-mute",
        };
        self.run(&[verb, device, if muted { "1" } else { "0" }])
            .await?;
        self.wait_for(direction, device, |snapshot| {
            let devices = match direction {
                AudioDirection::Output => &snapshot.outputs,
                AudioDirection::Input => &snapshot.inputs,
            };
            devices
                .iter()
                .any(|item| item.name == device && item.muted == muted)
        })
        .await
    }

    async fn set_default(
        &self,
        direction: AudioDirection,
        device: &str,
    ) -> Result<AudioSnapshot, AudioError> {
        let _guard = self.mutation_lock.lock().await;
        self.ensure_device(direction, device).await?;
        let verb = match direction {
            AudioDirection::Output => "set-default-sink",
            AudioDirection::Input => "set-default-source",
        };
        self.run(&[verb, device]).await?;
        self.wait_for(direction, device, |snapshot| match direction {
            AudioDirection::Output => snapshot.default_output.as_deref() == Some(device),
            AudioDirection::Input => snapshot.default_input.as_deref() == Some(device),
        })
        .await
    }

    async fn ensure_device(
        &self,
        direction: AudioDirection,
        device: &str,
    ) -> Result<(), AudioError> {
        let snapshot = self.snapshot().await?;
        let devices = match direction {
            AudioDirection::Output => &snapshot.outputs,
            AudioDirection::Input => &snapshot.inputs,
        };
        if devices.iter().any(|item| item.name == device) {
            Ok(())
        } else {
            Err(AudioError::DeviceUnavailable(device.to_owned()))
        }
    }

    async fn wait_for(
        &self,
        direction: AudioDirection,
        device: &str,
        mut matches: impl FnMut(&AudioSnapshot) -> bool,
    ) -> Result<AudioSnapshot, AudioError> {
        timeout(MUTATION_TIMEOUT, async {
            loop {
                let snapshot = self.snapshot().await?;
                let devices = match direction {
                    AudioDirection::Output => &snapshot.outputs,
                    AudioDirection::Input => &snapshot.inputs,
                };
                if !devices.iter().any(|item| item.name == device) {
                    return Err(AudioError::DeviceUnavailable(device.to_owned()));
                }
                if matches(&snapshot) {
                    return Ok(snapshot);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| AudioError::Timeout)?
    }

    async fn run(&self, arguments: &[&str]) -> Result<String, AudioError> {
        let output = timeout(
            COMMAND_TIMEOUT,
            Command::new(&self.executable)
                .args(arguments)
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .stdin(Stdio::null())
                .output(),
        )
        .await
        .map_err(|_| AudioError::Timeout)?
        .map_err(map_io_error)?;
        if output.stdout.len() > MAX_COMMAND_OUTPUT || output.stderr.len() > MAX_COMMAND_OUTPUT {
            return Err(AudioError::InvalidOutput(
                "Pactl output exceeded the size limit".to_owned(),
            ));
        }
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(map_command_error(error));
        }
        String::from_utf8(output.stdout)
            .map_err(|error| AudioError::InvalidOutput(error.to_string()))
    }

    async fn watch_task(&self, sender: mpsc::Sender<Result<AudioSnapshot, AudioError>>) {
        let mut retry_delay = Duration::from_millis(250);
        loop {
            if sender.is_closed() {
                return;
            }
            let child_result = Command::new(&self.executable)
                .arg("subscribe")
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let mut child = match child_result {
                Ok(child) => child,
                Err(error) => {
                    if sender.send(Err(map_io_error(error))).await.is_err() {
                        return;
                    }
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = (retry_delay * 2).min(Duration::from_secs(4));
                    continue;
                }
            };
            if sender.send(self.snapshot().await).await.is_err() {
                terminate_child(&mut child).await;
                return;
            }
            let Some(stdout) = child.stdout.take() else {
                let _ = sender.send(Err(AudioError::Disconnected)).await;
                return;
            };
            let mut lines = BufReader::new(stdout).lines();
            loop {
                tokio::select! {
                    _ = sender.closed() => {
                        terminate_child(&mut child).await;
                        return;
                    }
                    line = lines.next_line() => match line {
                        Ok(Some(_)) => {
                            retry_delay = Duration::from_millis(250);
                            if sender.send(self.snapshot().await).await.is_err() {
                                terminate_child(&mut child).await;
                                return;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
            terminate_child(&mut child).await;
            if sender.send(Err(AudioError::Disconnected)).await.is_err() {
                return;
            }
            tokio::time::sleep(retry_delay).await;
            retry_delay = (retry_delay * 2).min(Duration::from_secs(4));
        }
    }
}

impl Default for AudioClient {
    fn default() -> Self {
        Self::new()
    }
}

async fn terminate_child(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

fn map_io_error(error: io::Error) -> AudioError {
    match error.kind() {
        io::ErrorKind::NotFound => AudioError::MissingService("pactl executable not found".into()),
        io::ErrorKind::PermissionDenied => AudioError::PermissionDenied(error.to_string()),
        _ => AudioError::CommandFailed(error.to_string()),
    }
}

fn map_command_error(error: String) -> AudioError {
    let lower = error.to_ascii_lowercase();
    if lower.contains("access denied") || lower.contains("permission denied") {
        AudioError::PermissionDenied(error)
    } else if lower.contains("connection refused") || lower.contains("connection terminated") {
        AudioError::Disconnected
    } else {
        AudioError::CommandFailed(error)
    }
}

fn parse_defaults(info: &str) -> Result<(Option<String>, Option<String>), AudioError> {
    let mut output = None;
    let mut input = None;
    let mut has_output = false;
    let mut has_input = false;
    for line in info.lines() {
        if let Some(value) = line.trim().strip_prefix("Default Sink:") {
            has_output = true;
            output = nonempty(value);
        } else if let Some(value) = line.trim().strip_prefix("Default Source:") {
            has_input = true;
            input = nonempty(value);
        }
    }
    if !has_output || !has_input {
        return Err(AudioError::InvalidOutput(
            "Pactl info omitted default device fields".to_owned(),
        ));
    }
    Ok((output, input))
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_devices(
    output: &str,
    header: &str,
    direction: AudioDirection,
) -> Result<Vec<AudioDevice>, AudioError> {
    let mut devices = Vec::new();
    let mut current = None;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(index) = trimmed
            .strip_prefix(header)
            .and_then(|value| value.strip_prefix(" #"))
        {
            if let Some(device) = current.take() {
                devices.push(finish_device(device, direction)?);
            }
            let index = index
                .split_whitespace()
                .next()
                .ok_or_else(|| AudioError::InvalidOutput("missing device index".into()))?
                .parse()
                .map_err(|error| {
                    AudioError::InvalidOutput(format!("invalid device index: {error}"))
                })?;
            current = Some(PartialDevice {
                index,
                name: None,
                description: None,
                muted: None,
                volume_millipercent: None,
            });
            continue;
        }
        let Some(device) = current.as_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("Name:") {
            device.name = nonempty(value);
        } else if let Some(value) = trimmed.strip_prefix("Description:") {
            device.description = nonempty(value);
        } else if let Some(value) = trimmed.strip_prefix("Mute:") {
            device.muted = Some(match value.trim() {
                "yes" => true,
                "no" => false,
                other => {
                    return Err(AudioError::InvalidOutput(format!(
                        "invalid mute value: {other}"
                    )))
                }
            });
        } else if let Some(value) = trimmed.strip_prefix("Volume:") {
            device.volume_millipercent = Some(parse_volumes(value)?);
        }
    }
    if let Some(device) = current {
        devices.push(finish_device(device, direction)?);
    }
    if direction == AudioDirection::Input {
        devices.retain(|device| !device.name.ends_with(".monitor"));
    }
    Ok(devices)
}

/// `pactl list sink-inputs`: one block per application stream.
fn parse_streams(output: &str) -> Result<Vec<AudioStream>, AudioError> {
    let mut streams = Vec::new();
    let mut current: Option<PartialStream> = None;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(index) = trimmed.strip_prefix("Sink Input #") {
            if let Some(stream) = current.take() {
                streams.push(stream.finish()?);
            }
            let index = index.trim().parse().map_err(|error| {
                AudioError::InvalidOutput(format!("invalid stream index: {error}"))
            })?;
            current = Some(PartialStream {
                index,
                ..PartialStream::default()
            });
            continue;
        }
        let Some(stream) = current.as_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("Sink:") {
            stream.output = value.trim().parse().ok();
        } else if let Some(value) = trimmed.strip_prefix("Mute:") {
            stream.muted = match value.trim() {
                "yes" => Some(true),
                "no" => Some(false),
                other => {
                    return Err(AudioError::InvalidOutput(format!(
                        "invalid mute value: {other}"
                    )))
                }
            };
        } else if let Some(value) = trimmed.strip_prefix("Volume:") {
            stream.volume_millipercent = Some(parse_volumes(value)?);
        } else if let Some(value) = property(trimmed, "application.name") {
            stream.application = Some(value);
        } else if let Some(value) = property(trimmed, "application.process.binary") {
            stream.binary = Some(value);
        } else if let Some(value) = property(trimmed, "media.name") {
            stream.media = Some(value);
        }
    }
    if let Some(stream) = current {
        streams.push(stream.finish()?);
    }
    Ok(streams)
}

#[derive(Default)]
struct PartialStream {
    index: u32,
    output: Option<u32>,
    muted: Option<bool>,
    volume_millipercent: Option<Vec<u32>>,
    application: Option<String>,
    binary: Option<String>,
    media: Option<String>,
}

impl PartialStream {
    fn finish(self) -> Result<AudioStream, AudioError> {
        Ok(AudioStream {
            index: self.index,
            application: self
                .application
                .or(self.binary)
                .unwrap_or_else(|| format!("Stream {}", self.index)),
            media: self.media,
            output: self
                .output
                .ok_or_else(|| AudioError::InvalidOutput("stream omitted its sink".into()))?,
            muted: self
                .muted
                .ok_or_else(|| AudioError::InvalidOutput("stream omitted mute state".into()))?,
            volume_millipercent: self
                .volume_millipercent
                .ok_or_else(|| AudioError::InvalidOutput("stream omitted volume".into()))?,
        })
    }
}

/// `key = "value"` from a property list, bounded and non-empty.
fn property(line: &str, key: &str) -> Option<String> {
    let value = line
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?
        .trim();
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    (!value.is_empty()).then(|| value.chars().take(120).collect())
}

struct PartialDevice {
    index: u32,
    name: Option<String>,
    description: Option<String>,
    muted: Option<bool>,
    volume_millipercent: Option<Vec<u32>>,
}

fn finish_device(
    device: PartialDevice,
    direction: AudioDirection,
) -> Result<AudioDevice, AudioError> {
    Ok(AudioDevice {
        index: device.index,
        name: device
            .name
            .ok_or_else(|| AudioError::InvalidOutput("device omitted name".into()))?,
        description: device.description,
        direction,
        muted: device
            .muted
            .ok_or_else(|| AudioError::InvalidOutput("device omitted mute state".into()))?,
        volume_millipercent: device
            .volume_millipercent
            .ok_or_else(|| AudioError::InvalidOutput("device omitted volume".into()))?,
        is_default: false,
    })
}

fn parse_volumes(value: &str) -> Result<Vec<u32>, AudioError> {
    let volumes = value
        .split(',')
        .map(|channel| {
            let percent = channel
                .split('/')
                .nth(1)
                .ok_or_else(|| AudioError::InvalidOutput("malformed channel volume".into()))?
                .trim()
                .strip_suffix('%')
                .ok_or_else(|| AudioError::InvalidOutput("volume omitted percent".into()))?;
            parse_millipercent(percent)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if volumes.is_empty() {
        return Err(AudioError::InvalidOutput(
            "device has no volume channels".into(),
        ));
    }
    Ok(volumes)
}

fn parse_millipercent(value: &str) -> Result<u32, AudioError> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let whole = whole
        .parse::<u32>()
        .map_err(|error| AudioError::InvalidOutput(format!("invalid volume: {error}")))?;
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AudioError::InvalidOutput("invalid volume precision".into()));
    }
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u32>()
            .map_err(|error| AudioError::InvalidOutput(format!("invalid volume: {error}")))?
            * 10_u32.pow(3 - fraction.len() as u32)
    };
    whole
        .checked_mul(1000)
        .and_then(|whole| whole.checked_add(fraction_value))
        .ok_or_else(|| AudioError::InvalidOutput("volume overflow".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fixture_client() -> (AudioClient, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("output_volume"), "85").unwrap();
        std::fs::write(directory.path().join("input_volume"), "60").unwrap();
        std::fs::write(directory.path().join("output_mute"), "0").unwrap();
        std::fs::write(directory.path().join("input_mute"), "0").unwrap();
        std::fs::write(directory.path().join("default_output"), "sink.one").unwrap();
        std::fs::write(directory.path().join("default_input"), "source.one").unwrap();
        std::fs::write(directory.path().join("remove_sink_two"), "0").unwrap();
        std::fs::write(directory.path().join("subscription_attempts"), "0").unwrap();
        std::fs::write(directory.path().join("stream_volume"), "100").unwrap();
        std::fs::write(directory.path().join("stream_mute"), "no").unwrap();
        std::fs::write(directory.path().join("stream_sink"), "1").unwrap();
        let executable = directory.path().join("pactl");
        std::fs::write(
            &executable,
            r##"#!/bin/sh
set -eu
root=${0%/*}
        case "$1" in
  info)
    printf 'Server Name: PulseAudio (on PipeWire)\nDefault Sink: %s\nDefault Source: %s\n' "$(cat "$root/default_output")" "$(cat "$root/default_input")"
    ;;
  list)
    if [ "$2" = sink-inputs ]; then
      printf 'Sink Input #42\n\tDriver: protocol-native.c\n\tSink: %s\n\tMute: %s\n\tVolume: front-left: 65536 / %s%% / 0 dB, front-right: 65536 / %s%% / 0 dB\n\tProperties:\n\t\tmedia.name = "Playback"\n\t\tapplication.name = "Fixture Player"\n\t\tapplication.process.binary = "fixture"\n' "$(cat "$root/stream_sink")" "$(cat "$root/stream_mute")" "$(cat "$root/stream_volume")" "$(cat "$root/stream_volume")"
    elif [ "$2" = sinks ]; then
      for index in 1 2 3 4; do
        case "$index" in 1) name=sink.one ;; 2) name=sink.two ;; 3) name=sink.denied ;; *) name=sink.failed ;; esac
        [ "$name" != sink.two ] || [ "$(cat "$root/remove_sink_two")" != 2 ] || continue
        volume=$(cat "$root/output_volume")
        mute=$(cat "$root/output_mute")
        [ "$mute" = 1 ] && mute=yes || mute=no
        printf 'Sink #%s\n\tName: %s\n\tDescription: Fixture output %s\n\tMute: %s\n\tVolume: front-left: 65536 / %s%% / 0 dB, front-right: 65536 / %s%% / 0 dB\n' "$index" "$name" "$index" "$mute" "$volume" "$volume"
      done
    else
      printf 'Source #0\n\tName: sink.one.monitor\n\tMute: no\n\tVolume: mono: 65536 / 100%% / 0 dB\n'
      for index in 1 2; do
        name="source.$([ "$index" = 1 ] && printf one || printf two)"
        volume=$(cat "$root/input_volume")
        mute=$(cat "$root/input_mute")
        [ "$mute" = 1 ] && mute=yes || mute=no
        printf 'Source #%s\n\tName: %s\n\tDescription: Fixture input %s\n\tMute: %s\n\tVolume: mono: 65536 / %s%% / 0 dB\n' "$index" "$name" "$index" "$mute" "$volume"
      done
    fi
    ;;
  set-sink-volume)
    [ "$2" = sink.denied ] && { printf 'Failure: Access denied\n' >&2; exit 1; }
    [ "$2" = sink.failed ] && { printf 'Failure: device failure\n' >&2; exit 1; }
    if [ "$2" = sink.two ] && [ "$(cat "$root/remove_sink_two")" = 1 ]; then
      printf '2\n' > "$root/remove_sink_two"
      exit 0
    fi
    [ "$2" != sink.two ] || [ "$(cat "$root/remove_sink_two")" != 2 ] || exit 1
    printf '%s\n' "${3%%%}" > "$root/output_volume"
    ;;
  set-source-volume) printf '%s\n' "${3%%%}" > "$root/input_volume" ;;
  set-sink-mute) printf '%s\n' "$3" > "$root/output_mute" ;;
  set-source-mute) printf '%s\n' "$3" > "$root/input_mute" ;;
  set-default-sink) printf '%s\n' "$2" > "$root/default_output" ;;
  set-sink-input-volume) [ "$2" = 42 ] || exit 1; printf '%s\n' "${3%%%}" > "$root/stream_volume" ;;
  set-sink-input-mute) [ "$2" = 42 ] || exit 1; [ "$3" = 1 ] && printf yes > "$root/stream_mute" || printf no > "$root/stream_mute" ;;
  move-sink-input) [ "$2" = 42 ] || exit 1; [ "$3" = sink.two ] && printf 2 > "$root/stream_sink" ;;
  set-default-source) printf '%s\n' "$2" > "$root/default_input" ;;
  subscribe)
    attempts=$(cat "$root/subscription_attempts")
    printf '%s\n' "$((attempts + 1))" > "$root/subscription_attempts"
    [ "$attempts" != 0 ] || exit 0
    printf '90\n' > "$root/output_volume.$$.tmp"
    mv "$root/output_volume.$$.tmp" "$root/output_volume"
    printf "Event 'change' on sink #1\n"
    ;;
  *) printf 'unexpected command: %s\n' "$*" >&2; exit 2 ;;
esac
"##,
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        (
            AudioClient {
                executable,
                revision: Arc::new(AtomicU64::new(0)),
                mutation_lock: Arc::new(Mutex::new(())),
            },
            directory,
        )
    }

    #[test]
    fn sink_and_source_output_preserves_device_identity_state_and_channels() {
        let sinks = "Sink #3\n\tName: alsa_output.usb-test\n\tDescription: USB Speakers\n\tMute: no\n\tVolume: front-left: 32768 / 50.00% / -18.06 dB, front-right: 65536 / 100% / 0.00 dB\n";
        let sources = "Source #4\n\tName: alsa_input.usb-test\n\tDescription: USB Microphone\n\tMute: yes\n\tVolume: mono: 49152 / 75% / -7.50 dB\n";
        let outputs = parse_devices(sinks, "Sink", AudioDirection::Output).unwrap();
        let inputs = parse_devices(sources, "Source", AudioDirection::Input).unwrap();
        assert_eq!(outputs[0].name, "alsa_output.usb-test");
        assert_eq!(outputs[0].volume_millipercent, [50_000, 100_000]);
        assert!(!outputs[0].muted);
        assert_eq!(inputs[0].volume_millipercent, [75_000]);
        assert!(inputs[0].muted);
    }

    #[test]
    fn empty_devices_and_missing_default_are_not_replaced_with_fixtures() {
        assert!(parse_devices("", "Sink", AudioDirection::Output)
            .unwrap()
            .is_empty());
        assert_eq!(
            parse_defaults("Default Sink: \nDefault Source: \n").unwrap(),
            (None, None)
        );
    }

    #[test]
    fn malformed_device_state_fails_closed() {
        let malformed =
            "Sink #1\n\tName: output\n\tMute: maybe\n\tVolume: mono: 65536 / 100% / 0 dB\n";
        assert!(matches!(
            parse_devices(malformed, "Sink", AudioDirection::Output),
            Err(AudioError::InvalidOutput(_))
        ));
    }

    #[test]
    fn monitor_sources_are_not_reported_as_microphone_inputs() {
        let sources = "Source #0\n\tName: alsa_output.test.monitor\n\tMute: no\n\tVolume: mono: 65536 / 100% / 0 dB\nSource #1\n\tName: alsa_input.test\n\tMute: no\n\tVolume: mono: 65536 / 100% / 0 dB\n";
        let inputs = parse_devices(sources, "Source", AudioDirection::Input).unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name, "alsa_input.test");
    }

    #[test]
    fn availability_preserves_service_and_permission_states() {
        assert_eq!(
            AudioError::MissingService("pactl".into()).availability(),
            Availability::MissingService
        );
        assert_eq!(
            AudioError::PermissionDenied("denied".into()).availability(),
            Availability::PermissionDenied
        );
    }

    #[tokio::test]
    async fn pactl_fixture_exercises_snapshot_controls_readback_and_subscription() {
        let (client, _directory) = fixture_client();
        let initial = client.snapshot().await.unwrap();
        assert_eq!(initial.availability, Availability::Available);
        assert_eq!(initial.default_output.as_deref(), Some("sink.one"));
        assert_eq!(initial.default_input.as_deref(), Some("source.one"));
        assert_eq!(initial.outputs.len(), 4);
        assert_eq!(initial.inputs.len(), 2);
        assert!(!initial
            .inputs
            .iter()
            .any(|device| device.name.ends_with(".monitor")));

        let volume = client.set_output_volume("sink.two", 42).await.unwrap();
        assert_eq!(volume.outputs[1].volume_millipercent, [42_000, 42_000]);
        let muted = client.set_output_muted("sink.two", true).await.unwrap();
        assert!(muted.outputs[1].muted);
        let input_volume = client.set_input_volume("source.two", 33).await.unwrap();
        assert_eq!(input_volume.inputs[1].volume_millipercent, [33_000]);
        let input_muted = client.set_input_muted("source.two", true).await.unwrap();
        assert!(input_muted.inputs[1].muted);
        let default_output = client.set_default_output("sink.two").await.unwrap();
        assert_eq!(default_output.default_output.as_deref(), Some("sink.two"));
        let default_input = client.set_default_input("source.two").await.unwrap();
        assert_eq!(default_input.default_input.as_deref(), Some("source.two"));
        assert!(matches!(
            client.set_output_volume("sink.two", 151).await,
            Err(AudioError::InvalidVolume)
        ));
        assert!(matches!(
            client.set_output_volume("sink.denied", 50).await,
            Err(AudioError::PermissionDenied(_))
        ));
        assert!(matches!(
            client.set_output_volume("sink.failed", 50).await,
            Err(AudioError::CommandFailed(_))
        ));

        let mut events = client.watch();
        let first = timeout(Duration::from_secs(2), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let disconnected = timeout(Duration::from_secs(2), events.next())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(disconnected, Err(AudioError::Disconnected)));
        let mut reconnected = None;
        for _ in 0..3 {
            match timeout(Duration::from_secs(2), events.next())
                .await
                .unwrap()
                .unwrap()
            {
                Ok(snapshot) if snapshot.outputs[0].volume_millipercent == [90_000, 90_000] => {
                    reconnected = Some(snapshot);
                    break;
                }
                Ok(_) | Err(AudioError::Disconnected) => {}
                Err(error) => panic!("unexpected audio watcher failure: {error}"),
            }
        }
        let reconnected =
            reconnected.expect("watcher must reconnect and publish current audio state");
        assert!(reconnected.revision > first.revision);
        events.close().await;
    }

    #[tokio::test]
    async fn application_streams_are_listed_and_controlled() {
        let (client, directory) = fixture_client();
        let snapshot = client.snapshot().await.unwrap();
        assert_eq!(
            snapshot.streams,
            vec![AudioStream {
                index: 42,
                application: "Fixture Player".into(),
                media: Some("Playback".into()),
                output: 1,
                muted: false,
                volume_millipercent: vec![100_000, 100_000],
            }]
        );
        let changed = client.set_stream_volume(42, 40).await.unwrap();
        assert_eq!(changed.streams[0].volume_millipercent, vec![40_000, 40_000]);
        let changed = client.set_stream_muted(42, true).await.unwrap();
        assert!(changed.streams[0].muted);
        let moved = client.move_stream(42, "sink.two").await.unwrap();
        assert_eq!(moved.streams[0].output, 2);
        assert!(matches!(
            client.move_stream(42, "sink.none").await,
            Err(AudioError::DeviceUnavailable(_))
        ));
        assert!(matches!(
            client.set_stream_volume(7, 10).await,
            Err(AudioError::DeviceUnavailable(_))
        ));
        assert!(matches!(
            client.set_stream_volume(42, 151).await,
            Err(AudioError::InvalidVolume)
        ));
        drop(directory);
    }

    #[test]
    fn stream_blocks_without_names_or_with_bad_fields() {
        let unnamed = "Sink Input #3\n\tSink: 0\n\tMute: no\n\tVolume: mono: 65536 / 100% / 0 dB\n\tProperties:\n\t\tapplication.process.binary = \"mpv\"\n";
        assert_eq!(parse_streams(unnamed).unwrap()[0].application, "mpv");
        let bare = "Sink Input #4\n\tSink: 0\n\tMute: no\n\tVolume: mono: 65536 / 100% / 0 dB\n";
        assert_eq!(parse_streams(bare).unwrap()[0].application, "Stream 4");
        assert!(
            parse_streams("Sink Input #5\n\tMute: no\n\tVolume: mono: 1 / 1% / 0 dB\n").is_err()
        );
        assert!(parse_streams("Sink Input #x\n").is_err());
        assert!(parse_streams("").unwrap().is_empty());
    }

    #[tokio::test]
    async fn concurrent_default_volume_adjustments_are_serialized() {
        let (client, _directory) = fixture_client();
        let (first, second) = tokio::join!(
            client.adjust_default_output_volume(5),
            client.adjust_default_output_volume(5)
        );
        let first = first.unwrap();
        let second = second.unwrap();
        assert_eq!(first.outputs[0].volume_millipercent, [90_000, 90_000]);
        assert_eq!(second.outputs[0].volume_millipercent, [95_000, 95_000]);
    }

    #[tokio::test]
    async fn concurrent_default_mute_toggles_are_serialized() {
        let (client, _directory) = fixture_client();
        let (first, second) = tokio::join!(
            client.toggle_default_output_mute(),
            client.toggle_default_output_mute()
        );
        assert!(first.unwrap().outputs[0].muted);
        assert!(!second.unwrap().outputs[0].muted);
    }

    #[tokio::test]
    async fn mutation_reports_target_removed_during_readback() {
        let (client, directory) = fixture_client();
        std::fs::write(directory.path().join("remove_sink_two"), "1").unwrap();
        assert!(matches!(
            client.set_output_volume("sink.two", 42).await,
            Err(AudioError::DeviceUnavailable(name)) if name == "sink.two"
        ));
    }

    #[tokio::test]
    async fn missing_pactl_is_reported_without_fabricating_audio_state() {
        let client = AudioClient {
            executable: PathBuf::from("/no/such/pactl"),
            revision: Arc::new(AtomicU64::new(0)),
            mutation_lock: Arc::new(Mutex::new(())),
        };
        let error = client.snapshot().await.unwrap_err();
        assert_eq!(error.availability(), Availability::MissingService);
    }

    #[tokio::test]
    async fn pactl_command_timeout_kills_the_child_process() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("child.pid");
        let executable = directory.path().join("pactl");
        std::fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nexec sleep 30\n",
                pid_file.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let client = AudioClient {
            executable,
            revision: Arc::new(AtomicU64::new(0)),
            mutation_lock: Arc::new(Mutex::new(())),
        };

        let result = client.run(&[]).await;
        assert!(
            matches!(result, Err(AudioError::Timeout)),
            "unexpected timed-out command result: {result:?}"
        );
        let child_pid = std::fs::read_to_string(pid_file).unwrap();
        timeout(Duration::from_secs(1), async {
            loop {
                match std::fs::read_to_string(format!("/proc/{}/stat", child_pid.trim())) {
                    Ok(stat) => {
                        let state = stat
                            .rsplit_once(')')
                            .unwrap()
                            .1
                            .split_whitespace()
                            .next()
                            .unwrap();
                        if state == "Z" {
                            break;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                    Err(error) => panic!("could not inspect timed-out child: {error}"),
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("timed-out pactl child must stop running");
    }

    #[tokio::test]
    #[ignore = "requires a Pulse-compatible server, slopos_test_null sink, and remapped source"]
    async fn live_pulse_server_round_trips_only_the_named_null_sink() {
        let client = AudioClient::new();
        let initial = client.snapshot().await.unwrap();
        assert_eq!(initial.availability, Availability::Available);
        assert_eq!(initial.default_output.as_deref(), Some("slopos_test_null"));
        assert_eq!(initial.default_input.as_deref(), Some("slopos_test_input"));

        let volume = client
            .set_output_volume("slopos_test_null", 37)
            .await
            .unwrap();
        assert!(volume.outputs.iter().any(|output| {
            output.name == "slopos_test_null"
                && output
                    .volume_millipercent
                    .iter()
                    .all(|value| value.abs_diff(37_000) <= 1_000)
        }));

        let muted = client
            .set_output_muted("slopos_test_null", true)
            .await
            .unwrap();
        assert!(muted
            .outputs
            .iter()
            .any(|output| output.name == "slopos_test_null" && output.muted));
        let unmuted = client
            .set_output_muted("slopos_test_null", false)
            .await
            .unwrap();
        assert!(unmuted
            .outputs
            .iter()
            .any(|output| output.name == "slopos_test_null" && !output.muted));

        let input_volume = client
            .set_input_volume("slopos_test_input", 31)
            .await
            .unwrap();
        assert!(input_volume.inputs.iter().any(|input| {
            input.name == "slopos_test_input"
                && input
                    .volume_millipercent
                    .iter()
                    .all(|value| value.abs_diff(31_000) <= 1_000)
        }));
        let input_muted = client
            .set_input_muted("slopos_test_input", true)
            .await
            .unwrap();
        assert!(input_muted
            .inputs
            .iter()
            .any(|input| input.name == "slopos_test_input" && input.muted));
        let input_unmuted = client
            .set_input_muted("slopos_test_input", false)
            .await
            .unwrap();
        assert!(input_unmuted
            .inputs
            .iter()
            .any(|input| input.name == "slopos_test_input" && !input.muted));
    }
}
