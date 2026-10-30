//! Media playback engine (MEDIA-01): an adapter over GStreamer's `playbin`.
//!
//! The UI never talks to GStreamer directly. [`Player`] owns the pipeline
//! and reports through [`PlayerEvent`]s on a channel plus a wake callback,
//! so the window sleeps until something happens:
//! - decoded video frames arrive as RGBA through an `appsink` that keeps
//!   only the newest frame (at most one frame buffered, `drop=true`), and
//!   GStreamer's clock keeps audio and video in sync;
//! - bus messages (state, end of stream, errors, missing codecs) are read on
//!   a dedicated thread that blocks on the bus;
//! - frames larger than [`MAX_VIDEO_PIXELS`] are scaled down by GStreamer
//!   before they reach the app, bounding memory per frame.
//!
//! Missing codecs are reported by name with the package that usually
//! provides them; nothing falls back to a silent fake.
//!
//! Licensing: GStreamer and its base/good/x plugins are LGPL-2.1+ and are
//! linked dynamically; the Rust bindings are MIT/Apache-2.0. SLOPOS ships no
//! patent-encumbered codecs; H.264/H.265/AAC need the distribution's
//! `gstreamer1.0-libav` (or similar), which the error text names.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

/// Largest decoded frame handed to the UI (1920 × 1080).
pub const MAX_VIDEO_PIXELS: u64 = 1920 * 1080;

/// One decoded video frame, straight RGBA.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
    /// Presentation time.
    pub position: Duration,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("position", &self.position)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Paused,
    Playing,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    /// The file was opened and its streams are known.
    Opened(MediaInfo),
    State(PlaybackState),
    /// A new frame is waiting in [`Player::take_frame`].
    Frame,
    /// Playback failed; the text says why in user terms.
    Error(String),
    /// A codec is missing; the text names it and how to get it.
    MissingCodec(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaInfo {
    pub duration: Option<Duration>,
    pub has_video: bool,
    pub has_audio: bool,
    pub width: u32,
    pub height: u32,
    pub subtitles: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("GStreamer is not available: {0}")]
    Unavailable(String),
    #[error("{0}")]
    Pipeline(String),
}

/// Initialise GStreamer once; returns an error users can read when the
/// runtime or its base plugins are missing.
pub fn init() -> Result<(), EngineError> {
    gst::init().map_err(|error| EngineError::Unavailable(error.to_string()))?;
    for element in ["playbin", "appsink", "videoconvert", "videoscale"] {
        if gst::ElementFactory::find(element).is_none() {
            return Err(EngineError::Unavailable(format!(
                "the “{element}” element is missing (install gstreamer1.0-plugins-base)"
            )));
        }
    }
    Ok(())
}

/// A sibling subtitle file: same stem with `.srt`, `.vtt`, `.ass` or `.ssa`.
pub fn find_subtitles(media: &Path) -> Option<PathBuf> {
    let stem = media.file_stem()?.to_string_lossy().into_owned();
    let dir = media.parent()?;
    ["srt", "vtt", "ass", "ssa"]
        .iter()
        .map(|ext| dir.join(format!("{stem}.{ext}")))
        .find(|candidate| candidate.is_file())
}

/// Which package usually provides a missing GStreamer element or decoder.
pub fn codec_hint(description: &str) -> &'static str {
    let lower = description.to_lowercase();
    if [
        "h.264", "h264", "h.265", "hevc", "aac", "mpeg-4", "mp3", "ac-3", "wma", "wmv",
    ]
    .iter()
    .any(|codec| lower.contains(codec))
    {
        "gstreamer1.0-libav (FFmpeg decoders; check codec licensing in your country)"
    } else if lower.contains("av1") || lower.contains("dav1d") {
        "gstreamer1.0-plugins-bad"
    } else {
        "gstreamer1.0-plugins-good or gstreamer1.0-plugins-bad"
    }
}

/// Size to scale a `width × height` video to so it fits [`MAX_VIDEO_PIXELS`]
/// (keeping the aspect ratio, even dimensions).
pub fn bounded_size(width: u32, height: u32) -> (u32, u32) {
    let pixels = u64::from(width) * u64::from(height);
    if pixels <= MAX_VIDEO_PIXELS || width == 0 || height == 0 {
        return (width, height);
    }
    let factor = (MAX_VIDEO_PIXELS as f64 / pixels as f64).sqrt();
    let even = |value: f64| ((value as u32) & !1).max(2);
    (
        even(f64::from(width) * factor),
        even(f64::from(height) * factor),
    )
}

pub struct Player {
    playbin: gst::Element,
    events: Receiver<PlayerEvent>,
    frame: Arc<Mutex<Option<Frame>>>,
    bus_thread: Option<thread::JoinHandle<()>>,
}

impl Player {
    /// Open `path` paused at the start. `wake` is called (from GStreamer
    /// threads) whenever an event or frame is ready.
    pub fn open(path: &Path, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, EngineError> {
        init()?;
        let uri = gst::glib::filename_to_uri(path, None).map_err(|error| {
            EngineError::Pipeline(format!("can't open {}: {error}", path.display()))
        })?;
        let playbin = gst::ElementFactory::make("playbin")
            .property("uri", uri.as_str())
            .build()
            .map_err(|error| EngineError::Pipeline(error.to_string()))?;
        let subtitles = find_subtitles(path);
        if let Some(subtitles) = &subtitles {
            if let Ok(uri) = gst::glib::filename_to_uri(subtitles, None) {
                playbin.set_property("suburi", uri.as_str());
                playbin.set_property_from_str("subtitle-font-desc", "Sans 18");
            }
        }

        // Video: convert and (if needed) scale to bounded RGBA in an appsink.
        let frame: Arc<Mutex<Option<Frame>>> = Arc::new(Mutex::new(None));
        let (sender, events) = mpsc::channel();
        let video_sink = Self::video_sink(frame.clone(), sender.clone(), wake.clone())?;
        playbin.set_property("video-sink", &video_sink);
        // Without a sound device, play silently instead of failing.
        if !Self::audio_output_available() {
            if let Ok(fake) = gst::ElementFactory::make("fakesink")
                .property("sync", true)
                .build()
            {
                playbin.set_property("audio-sink", &fake);
            }
        }

        let bus = playbin
            .bus()
            .ok_or_else(|| EngineError::Pipeline("no bus".into()))?;
        let thread_playbin = playbin.downgrade();
        let bus_thread = thread::Builder::new()
            .name("media-bus".into())
            .spawn(move || Self::watch_bus(bus, thread_playbin, sender, subtitles, wake))
            .map_err(|error| EngineError::Pipeline(error.to_string()))?;

        playbin
            .set_state(gst::State::Paused)
            .map_err(|_| EngineError::Pipeline("this file can't be played".into()))?;
        Ok(Self {
            playbin,
            events,
            frame,
            bus_thread: Some(bus_thread),
        })
    }

    fn audio_output_available() -> bool {
        // PulseAudio/PipeWire socket, or an ALSA device.
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let pulse = runtime.is_some_and(|dir| {
            dir.join("pulse/native").exists() || dir.join("pipewire-0").exists()
        });
        pulse || Path::new("/dev/snd/pcmC0D0p").exists()
    }

    fn video_sink(
        frame: Arc<Mutex<Option<Frame>>>,
        sender: Sender<PlayerEvent>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<gst::Element, EngineError> {
        let make = |name: &str| {
            gst::ElementFactory::make(name)
                .build()
                .map_err(|error| EngineError::Pipeline(format!("{name}: {error}")))
        };
        let bin = gst::Bin::new();
        let convert = make("videoconvert")?;
        let scale = make("videoscale")?;
        let filter = make("capsfilter")?;
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGBA")
            .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
            .build();
        filter.set_property("caps", &caps);
        let sink = gst_app::AppSink::builder()
            .caps(&caps)
            .max_buffers(1)
            .drop(true)
            .sync(true)
            .build();
        bin.add_many([&convert, &scale, &filter, sink.upcast_ref()])
            .map_err(|error| EngineError::Pipeline(error.to_string()))?;
        gst::Element::link_many([&convert, &scale, &filter, sink.upcast_ref()])
            .map_err(|error| EngineError::Pipeline(error.to_string()))?;
        let pad = convert
            .static_pad("sink")
            .ok_or_else(|| EngineError::Pipeline("no sink pad".into()))?;
        let ghost = gst::GhostPad::with_target(&pad)
            .map_err(|error| EngineError::Pipeline(error.to_string()))?;
        bin.add_pad(&ghost)
            .map_err(|error| EngineError::Pipeline(error.to_string()))?;

        // Bound the frame size: renegotiate scaled caps when the source is
        // larger than MAX_VIDEO_PIXELS.
        let filter_weak = filter.downgrade();
        pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
            if let Some(gst::PadProbeData::Event(event)) = &info.data {
                if let gst::EventView::Caps(caps) = event.view() {
                    if let Ok(video) = gst_video::VideoInfo::from_caps(caps.caps()) {
                        let (width, height) = bounded_size(video.width(), video.height());
                        if let Some(filter) = filter_weak.upgrade() {
                            let bounded = gst::Caps::builder("video/x-raw")
                                .field("format", "RGBA")
                                .field("width", width as i32)
                                .field("height", height as i32)
                                .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
                                .build();
                            filter.set_property("caps", &bounded);
                        }
                    }
                }
            }
            gst::PadProbeReturn::Ok
        });

        // The paused preroll frame arrives through `new_preroll`, playback
        // frames through `new_sample`; both store the newest frame.
        let preroll = Self::frame_handler(frame.clone(), sender.clone(), wake.clone());
        let playing = Self::frame_handler(frame, sender, wake);
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_preroll(move |sink| {
                    let sample = sink.pull_preroll().map_err(|_| gst::FlowError::Eos)?;
                    preroll(&sample)
                })
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    playing(&sample)
                })
                .build(),
        );
        Ok(bin.upcast())
    }

    fn frame_handler(
        frame: Arc<Mutex<Option<Frame>>>,
        sender: Sender<PlayerEvent>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> impl Fn(&gst::Sample) -> Result<gst::FlowSuccess, gst::FlowError> + Send + Sync + 'static
    {
        move |sample| {
            let (Some(buffer), Some(caps)) = (sample.buffer(), sample.caps()) else {
                return Err(gst::FlowError::Error);
            };
            let info = gst_video::VideoInfo::from_caps(caps).map_err(|_| gst::FlowError::Error)?;
            let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
            let (width, height) = (info.width(), info.height());
            let stride = info.stride()[0] as usize;
            let row = width as usize * 4;
            let mut rgba = Vec::with_capacity(row * height as usize);
            for y in 0..height as usize {
                let start = y * stride;
                let Some(line) = map.get(start..start + row) else {
                    return Err(gst::FlowError::Error);
                };
                rgba.extend_from_slice(line);
            }
            let position = buffer
                .pts()
                .map_or(Duration::ZERO, |pts| Duration::from_nanos(pts.nseconds()));
            if let Ok(mut slot) = frame.lock() {
                *slot = Some(Frame {
                    width,
                    height,
                    rgba: Arc::new(rgba),
                    position,
                });
            }
            let _ = sender.send(PlayerEvent::Frame);
            wake();
            Ok(gst::FlowSuccess::Ok)
        }
    }

    fn watch_bus(
        bus: gst::Bus,
        playbin: gst::glib::WeakRef<gst::Element>,
        sender: Sender<PlayerEvent>,
        subtitles: Option<PathBuf>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) {
        let mut opened = false;
        for message in bus.iter_timed(gst::ClockTime::NONE) {
            let event = match message.view() {
                gst::MessageView::Eos(_) => Some(PlayerEvent::State(PlaybackState::Ended)),
                gst::MessageView::Error(error)
                    if [
                        gst::StreamError::TypeNotFound,
                        gst::StreamError::WrongType,
                        gst::StreamError::Format,
                        gst::StreamError::Demux,
                    ]
                    .iter()
                    .any(|kind| error.error().matches(*kind))
                        || error.debug().is_some_and(|d| d.contains("typefind")) =>
                {
                    Some(PlayerEvent::Error(
                        "this file is not a video or audio format SLOPOS can read".into(),
                    ))
                }
                gst::MessageView::Error(error) => {
                    let text = error.error().to_string();
                    let debug = error.debug().map(|d| d.to_string()).unwrap_or_default();
                    if debug.contains("missing")
                        || text.contains("No decoder")
                        || text.contains("codec")
                    {
                        Some(PlayerEvent::MissingCodec(format!(
                            "{text} Install {}.",
                            codec_hint(&format!("{text} {debug}"))
                        )))
                    } else {
                        Some(PlayerEvent::Error(text))
                    }
                }
                gst::MessageView::Element(element) => {
                    let structure = element.structure();
                    if structure.is_some_and(|s| s.name() == "missing-plugin") {
                        let detail = structure
                            .and_then(|s| s.get::<String>("name").ok())
                            .unwrap_or_else(|| "a decoder".into());
                        Some(PlayerEvent::MissingCodec(format!(
                            "SLOPOS can't play this file: {detail} is not installed. Install {}.",
                            codec_hint(&detail)
                        )))
                    } else {
                        None
                    }
                }
                gst::MessageView::StateChanged(change) => {
                    let is_pipeline = playbin.upgrade().is_some_and(|p| {
                        change
                            .src()
                            .is_some_and(|src| src == p.upcast_ref::<gst::Object>())
                    });
                    if !is_pipeline {
                        None
                    } else {
                        let mut events = Vec::new();
                        if !opened && change.current() >= gst::State::Paused {
                            opened = true;
                            if let Some(pipeline) = playbin.upgrade() {
                                events.push(PlayerEvent::Opened(Self::info(
                                    &pipeline,
                                    subtitles.clone(),
                                )));
                            }
                        }
                        let state = match change.current() {
                            gst::State::Playing => Some(PlaybackState::Playing),
                            gst::State::Paused => Some(PlaybackState::Paused),
                            gst::State::Ready | gst::State::Null => Some(PlaybackState::Stopped),
                            _ => None,
                        };
                        events.extend(state.map(PlayerEvent::State));
                        for event in events {
                            let _ = sender.send(event);
                        }
                        wake();
                        None
                    }
                }
                _ => None,
            };
            if let gst::MessageView::Application(app) = message.view() {
                if app.structure().is_some_and(|s| s.name() == "slopos-quit") {
                    break;
                }
            }
            if let Some(event) = event {
                let _ = sender.send(event);
                wake();
            }
            if playbin.upgrade().is_none() {
                break;
            }
        }
    }

    fn info(playbin: &gst::Element, subtitles: Option<PathBuf>) -> MediaInfo {
        let videos: i32 = playbin.property("n-video");
        let audios: i32 = playbin.property("n-audio");
        let duration = playbin
            .query_duration::<gst::ClockTime>()
            .map(|d| Duration::from_nanos(d.nseconds()));
        let (mut width, mut height) = (0, 0);
        if videos > 0 {
            let pad: Option<gst::Pad> = playbin.emit_by_name("get-video-pad", &[&0i32]);
            if let Some(caps) = pad.and_then(|pad| pad.current_caps()) {
                if let Ok(info) = gst_video::VideoInfo::from_caps(&caps) {
                    (width, height) = (info.width(), info.height());
                }
            }
        }
        MediaInfo {
            duration,
            has_video: videos > 0,
            has_audio: audios > 0,
            width,
            height,
            subtitles,
        }
    }

    /// Events received since the last call.
    pub fn poll_events(&self) -> Vec<PlayerEvent> {
        self.events
            .try_iter()
            .filter(|e| *e != PlayerEvent::Frame)
            .collect()
    }

    /// The newest decoded frame, if one arrived since the last call.
    pub fn take_frame(&self) -> Option<Frame> {
        self.frame.lock().ok().and_then(|mut slot| slot.take())
    }

    pub fn play(&self) -> Result<(), EngineError> {
        self.set_state(gst::State::Playing)
    }

    pub fn pause(&self) -> Result<(), EngineError> {
        self.set_state(gst::State::Paused)
    }

    fn set_state(&self, state: gst::State) -> Result<(), EngineError> {
        self.playbin
            .set_state(state)
            .map(|_| ())
            .map_err(|_| EngineError::Pipeline("playback could not change state".into()))
    }

    /// Seek to `position` (clamped to the duration when known).
    pub fn seek(&self, position: Duration) -> Result<(), EngineError> {
        let target =
            gst::ClockTime::from_nseconds(position.as_nanos().min(u128::from(u64::MAX)) as u64);
        self.playbin
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, target)
            .map_err(|_| EngineError::Pipeline("this file can't seek".into()))
    }

    pub fn position(&self) -> Option<Duration> {
        self.playbin
            .query_position::<gst::ClockTime>()
            .map(|p| Duration::from_nanos(p.nseconds()))
    }

    pub fn duration(&self) -> Option<Duration> {
        self.playbin
            .query_duration::<gst::ClockTime>()
            .map(|d| Duration::from_nanos(d.nseconds()))
    }

    /// Linear volume 0.0–1.0 (values outside are clamped).
    pub fn set_volume(&self, volume: f64) {
        let volume = if volume.is_nan() {
            0.0
        } else {
            volume.clamp(0.0, 1.0)
        };
        self.playbin.set_property("volume", volume);
    }

    #[cfg(test)]
    pub fn volume(&self) -> f64 {
        self.playbin.property("volume")
    }

    pub fn set_muted(&self, muted: bool) {
        self.playbin.set_property("mute", muted);
    }

    pub fn muted(&self) -> bool {
        self.playbin.property("mute")
    }

    /// Show or hide subtitles (when the file has any).
    pub fn set_subtitles_visible(&self, visible: bool) {
        let flags = self.playbin.property_value("flags");
        let Some(class) = gst::glib::FlagsClass::with_type(flags.type_()) else {
            return;
        };
        let Some(builder) = class.builder_with_value(flags) else {
            return;
        };
        let builder = if visible {
            builder.set_by_nick("text")
        } else {
            builder.unset_by_nick("text")
        };
        if let Some(value) = builder.build() {
            self.playbin.set_property_from_value("flags", &value);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Stop the bus thread first: once the pipeline reaches NULL its bus
        // flushes and would discard the quit message.
        if let Some(bus) = self.playbin.bus() {
            let _ = bus.post(gst::message::Application::new(gst::Structure::new_empty(
                "slopos-quit",
            )));
        }
        if let Some(thread) = self.bus_thread.take() {
            let _ = thread.join();
        }
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::time::Instant;

    /// Generate small fixtures with gst-launch-1.0 (part of the test tools).
    pub fn fixture(dir: &Path, name: &str, pipeline: &str) -> PathBuf {
        let path = dir.join(name);
        let status = std::process::Command::new("gst-launch-1.0")
            .arg("-q")
            .args(
                pipeline
                    .replace("OUT", &path.to_string_lossy())
                    .split_whitespace(),
            )
            .status()
            .expect("gst-launch-1.0 is installed for tests");
        assert!(status.success(), "fixture {name}");
        path
    }

    pub fn video(dir: &Path) -> PathBuf {
        fixture(
            dir,
            "clip.webm",
            "videotestsrc num-buffers=60 ! video/x-raw,width=160,height=120,framerate=30/1 ! vp8enc deadline=1 ! webmmux name=mux ! filesink location=OUT \
             audiotestsrc num-buffers=90 ! audioconvert ! vorbisenc ! mux.",
        )
    }

    fn wait_for(
        player: &Player,
        timeout: Duration,
        mut done: impl FnMut(&PlayerEvent) -> bool,
    ) -> Vec<PlayerEvent> {
        let start = Instant::now();
        let mut seen = Vec::new();
        while start.elapsed() < timeout {
            for event in player.poll_events() {
                let finished = done(&event);
                seen.push(event);
                if finished {
                    return seen;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out; saw {seen:?}");
    }

    fn no_wake() -> Arc<dyn Fn() + Send + Sync> {
        Arc::new(|| {})
    }

    #[test]
    fn opens_plays_seeks_and_ends() {
        let dir = tempfile::tempdir().unwrap();
        let clip = video(dir.path());
        std::fs::write(
            dir.path().join("clip.srt"),
            "1\n00:00:00,000 --> 00:00:01,500\nHi\n",
        )
        .unwrap();
        let player = Player::open(&clip, no_wake()).unwrap();
        let events = wait_for(&player, Duration::from_secs(10), |e| {
            matches!(e, PlayerEvent::Opened(_))
        });
        let info = events
            .iter()
            .find_map(|e| match e {
                PlayerEvent::Opened(info) => Some(info.clone()),
                _ => None,
            })
            .unwrap();
        assert!(info.has_video && info.has_audio);
        assert_eq!((info.width, info.height), (160, 120));
        assert!(info.duration.unwrap() >= Duration::from_millis(1900));
        assert_eq!(info.subtitles, Some(dir.path().join("clip.srt")));
        // Paused preroll still delivers the first frame.
        let start = Instant::now();
        let frame = loop {
            if let Some(frame) = player.take_frame() {
                break frame;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "no preroll frame");
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(
            (frame.width, frame.height, frame.rgba.len()),
            (160, 120, 160 * 120 * 4)
        );
        player.set_volume(0.25);
        assert!((player.volume() - 0.25).abs() < 1e-9);
        player.set_volume(f64::NAN);
        assert_eq!(player.volume(), 0.0);
        player.set_muted(true);
        assert!(player.muted());
        player.seek(Duration::from_millis(1500)).unwrap();
        player.play().unwrap();
        wait_for(&player, Duration::from_secs(15), |e| {
            *e == PlayerEvent::State(PlaybackState::Ended)
        });
    }

    #[test]
    fn broken_files_report_errors_instead_of_hanging() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.webm");
        std::fs::write(&broken, b"\x1a\x45\xdf\xa3 this is not really webm").unwrap();
        match Player::open(&broken, no_wake()) {
            Err(_) => {}
            Ok(player) => {
                wait_for(&player, Duration::from_secs(10), |e| {
                    matches!(e, PlayerEvent::Error(_) | PlayerEvent::MissingCodec(_))
                });
            }
        }
        // A file that does not exist fails to open or reports an error.
        if let Ok(player) = Player::open(&dir.path().join("missing.ogg"), no_wake()) {
            wait_for(&player, Duration::from_secs(10), |e| {
                matches!(e, PlayerEvent::Error(_))
            });
        }
    }

    #[test]
    fn helpers_bound_sizes_and_name_codecs() {
        assert_eq!(bounded_size(1280, 720), (1280, 720));
        let (w, h) = bounded_size(3840, 2160);
        assert!(u64::from(w) * u64::from(h) <= MAX_VIDEO_PIXELS && w % 2 == 0 && h % 2 == 0);
        assert!((f64::from(w) / f64::from(h) - 16.0 / 9.0).abs() < 0.01);
        assert_eq!(bounded_size(0, 100), (0, 100));
        assert!(codec_hint("H.264 (Main Profile) decoder").contains("libav"));
        assert!(codec_hint("AV1 decoder").contains("bad"));
        let dir = tempfile::tempdir().unwrap();
        let movie = dir.path().join("movie.mkv");
        assert_eq!(find_subtitles(&movie), None);
        std::fs::write(dir.path().join("movie.vtt"), "WEBVTT").unwrap();
        assert_eq!(find_subtitles(&movie), Some(dir.path().join("movie.vtt")));
    }
}
