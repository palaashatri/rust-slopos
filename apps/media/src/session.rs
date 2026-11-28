//! Playback state for the Media window, as plain logic.
//!
//! [`Session`] folds the engine's [`PlayerEvent`]s into what the window
//! shows: the transport state, the time, the chosen subtitle track and, when
//! playback can't go on, a [`Failure`] that says why. The window makes the
//! engine calls; the rules live here so they are tested without GStreamer,
//! a display or any codec.

use std::io;
use std::path::Path;
use std::time::Duration;

use crate::engine::{MediaInfo, PlaybackState, PlayerEvent};

/// Seconds moved by the Back and Forward commands.
pub const SEEK_STEP: f64 = 5.0;
/// Volume percentage moved by the Louder and Quieter commands.
pub const VOLUME_STEP: f64 = 5.0;
/// Volume when the first file opens (percent).
pub const DEFAULT_VOLUME: f64 = 80.0;
/// The subtitle choice that turns subtitles off.
pub const SUBTITLES_OFF: &str = "Off";
/// Shown, disabled, when the file has no text streams.
pub const NO_SUBTITLES: &str = "No subtitles";

/// Why playback can't go on. The window shows the title and message in the
/// video area and keeps Open… available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// GStreamer or its base plugins are not installed.
    Unavailable,
    /// The file is missing, is a folder, is not readable, or is not a format
    /// SLOPOS identifies.
    Unreadable,
    /// The format is known, but a decoder for one of its streams is not.
    MissingCodec,
    /// The file opened but has no audio or video stream.
    NoStreams,
    /// Playback started and then failed.
    Playback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    /// What the user can read: why playback failed and what to do next.
    pub message: String,
}

impl Failure {
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// The heading shown above the message.
    pub fn title(&self) -> &'static str {
        match self.kind {
            FailureKind::Unavailable => "Media playback is unavailable",
            FailureKind::Unreadable => "This file can't be played",
            FailureKind::MissingCodec => "A decoder for this file is missing",
            FailureKind::NoStreams => "There is nothing to play",
            FailureKind::Playback => "Playback stopped",
        }
    }
}

/// The failure an engine event reports, if it reports one.
pub fn failure_for(event: &PlayerEvent) -> Option<Failure> {
    match event {
        PlayerEvent::Unreadable(text) => Some(Failure::new(FailureKind::Unreadable, text)),
        PlayerEvent::MissingCodec(text) => Some(Failure::new(FailureKind::MissingCodec, text)),
        PlayerEvent::Error(text) => Some(Failure::new(FailureKind::Playback, text)),
        PlayerEvent::Opened(info) if !info.has_video && !info.has_audio => Some(Failure::new(
            FailureKind::NoStreams,
            "This file has no audio or video stream that SLOPOS can play.",
        )),
        PlayerEvent::Opened(_) | PlayerEvent::State(_) | PlayerEvent::Frame => None,
    }
}

/// The message for a file that could not be opened for reading.
pub fn io_message(name: &str, kind: io::ErrorKind) -> String {
    match kind {
        io::ErrorKind::NotFound => format!("“{name}” does not exist."),
        io::ErrorKind::PermissionDenied => {
            format!("SLOPOS isn't allowed to read “{name}”. Check its permissions.")
        }
        _ => format!("“{name}” can't be read."),
    }
}

/// Checks that `path` is a readable file before GStreamer is asked to decode
/// it, so a missing or locked file gets a plain reason instead of a
/// pipeline error.
pub fn check_readable(path: &Path) -> Result<(), Failure> {
    let name = file_name(path);
    if path.is_dir() {
        return Err(Failure::new(
            FailureKind::Unreadable,
            format!("“{name}” is a folder. Choose a media file instead."),
        ));
    }
    match std::fs::File::open(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(Failure::new(
            FailureKind::Unreadable,
            io_message(&name, error.kind()),
        )),
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// `m:ss`, or `h:mm:ss` from an hour on.
pub fn format_time(time: Duration) -> String {
    let seconds = time.as_secs();
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

/// What the Play button does in the current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Play,
    Pause,
    /// The file finished: start again from the beginning.
    Replay,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// A player exists for the current file (controls are live).
    pub loaded: bool,
    /// The engine reported the file's streams.
    pub opened: bool,
    pub state: PlaybackState,
    pub position: Duration,
    pub info: MediaInfo,
    pub failure: Option<Failure>,
    /// Volume in percent, 0 to 100. Kept across files.
    pub volume: f64,
    /// Index into `info.subtitle_tracks`; `None` when subtitles are off.
    pub subtitle: Option<usize>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            loaded: false,
            opened: false,
            state: PlaybackState::Stopped,
            position: Duration::ZERO,
            info: MediaInfo::default(),
            failure: None,
            volume: DEFAULT_VOLUME,
            subtitle: None,
        }
    }
}

impl Session {
    /// Start loading a file: everything but the volume starts afresh.
    pub fn begin(&mut self) {
        *self = Self {
            loaded: true,
            volume: self.volume,
            ..Self::default()
        };
    }

    /// Playback can't go on: keep the reason, drop the player.
    pub fn fail(&mut self, failure: Failure) {
        self.loaded = false;
        self.opened = false;
        self.state = PlaybackState::Stopped;
        self.failure = Some(failure);
    }

    /// Fold one engine event in. Failures are kept; the caller drops the
    /// player when [`Session::failure`] is set.
    pub fn apply(&mut self, event: &PlayerEvent) {
        if let Some(failure) = failure_for(event) {
            // The first failure is the cause; later pipeline errors follow it.
            if self.failure.is_none() {
                self.fail(failure);
            }
            return;
        }
        match event {
            PlayerEvent::Opened(info) => {
                self.opened = true;
                self.info = info.clone();
                // Subtitles start on when the file has a track to show.
                self.subtitle = (!self.info.subtitle_tracks.is_empty()).then_some(0);
            }
            PlayerEvent::State(state) => self.state = *state,
            PlayerEvent::Frame => {}
            PlayerEvent::Unreadable(_) | PlayerEvent::MissingCodec(_) | PlayerEvent::Error(_) => {}
        }
    }

    /// The transport action for the Play button, or `None` with no file.
    pub fn transport(&self) -> Option<Transport> {
        if !self.loaded {
            return None;
        }
        Some(match self.state {
            PlaybackState::Playing => Transport::Pause,
            PlaybackState::Ended => Transport::Replay,
            PlaybackState::Paused | PlaybackState::Stopped => Transport::Play,
        })
    }

    pub fn play_label(&self) -> &'static str {
        if self.state == PlaybackState::Playing {
            "Pause"
        } else {
            "Play"
        }
    }

    /// Where a seek by `delta` seconds lands: never before the start, and
    /// never past the end when the duration is known.
    pub fn seek_target(&self, delta: f64) -> Duration {
        let target = (self.position.as_secs_f64() + delta).max(0.0);
        let target = self
            .info
            .duration
            .map_or(target, |duration| target.min(duration.as_secs_f64()));
        Duration::from_secs_f64(target)
    }

    /// A seek finished at `target`. A seek back from the end resumes paused,
    /// so the Play button offers Play rather than Replay.
    pub fn seeked(&mut self, target: Duration) {
        self.position = target;
        let before_end = match self.info.duration {
            Some(duration) => target < duration,
            None => true,
        };
        if self.state == PlaybackState::Ended && before_end {
            self.state = PlaybackState::Paused;
        }
    }

    /// Clamp a percentage to 0 to 100; NaN becomes silence.
    pub fn clamp_volume(percent: f64) -> f64 {
        if percent.is_nan() {
            0.0
        } else {
            percent.clamp(0.0, 100.0)
        }
    }

    pub fn set_volume(&mut self, percent: f64) {
        self.volume = Self::clamp_volume(percent);
    }

    /// The choices for the subtitle pop-up and the selected one. With no
    /// text streams it has a single disabled entry.
    pub fn subtitle_options(&self) -> (Vec<String>, Option<usize>) {
        if self.info.subtitle_tracks.is_empty() {
            return (vec![NO_SUBTITLES.to_owned()], Some(0));
        }
        let mut items = vec![SUBTITLES_OFF.to_owned()];
        items.extend(self.info.subtitle_tracks.iter().cloned());
        (items, Some(self.subtitle.map_or(0, |track| track + 1)))
    }

    pub fn has_subtitles(&self) -> bool {
        !self.info.subtitle_tracks.is_empty()
    }

    /// Choose pop-up entry `option` (0 is Off). Out-of-range entries are
    /// ignored and return `false`.
    pub fn choose_subtitle_option(&mut self, option: usize) -> bool {
        let tracks = self.info.subtitle_tracks.len();
        match option {
            0 => self.subtitle = None,
            option if option <= tracks => self.subtitle = Some(option - 1),
            _ => return false,
        }
        true
    }

    /// The Playback menu's Show Subtitles: on the first track, or off.
    pub fn toggle_subtitles(&mut self) {
        self.subtitle = match self.subtitle {
            Some(_) => None,
            None if self.has_subtitles() => Some(0),
            None => None,
        };
    }

    /// The text under the video, e.g. "Playing — clip.webm — 640 × 360
    /// video with sound — 0:12".
    pub fn status_line(&self, name: &str) -> String {
        if let Some(failure) = &self.failure {
            return failure.title().to_owned();
        }
        if !self.opened {
            return format!("Opening “{name}”…");
        }
        let duration = self.info.duration.map_or_else(|| "—".into(), format_time);
        let kind = match (self.info.has_video, self.info.has_audio) {
            (true, true) => format!(
                "{} × {} video with sound",
                self.info.width, self.info.height
            ),
            (true, false) => format!("{} × {} video, no sound", self.info.width, self.info.height),
            (false, true) => "audio".into(),
            (false, false) => "no playable streams".into(),
        };
        let state = match self.state {
            PlaybackState::Playing => "Playing",
            PlaybackState::Paused => "Paused",
            PlaybackState::Ended => "Finished",
            PlaybackState::Stopped => "Stopped",
        };
        format!("{state} — {name} — {kind} — {duration}")
    }

    /// The seek bar's spoken value: "0:05 of 1:00".
    pub fn seek_value_text(&self) -> String {
        format!(
            "{} of {}",
            format_time(self.position),
            self.info
                .duration
                .map_or_else(|| "unknown".into(), format_time)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn info(video: bool, audio: bool, tracks: &[&str]) -> MediaInfo {
        MediaInfo {
            duration: Some(Duration::from_secs(60)),
            has_video: video,
            has_audio: audio,
            width: 640,
            height: 360,
            subtitles: None,
            subtitle_tracks: tracks.iter().map(|t| (*t).to_owned()).collect(),
        }
    }

    fn opened(tracks: &[&str]) -> Session {
        let mut session = Session::default();
        session.begin();
        session.apply(&PlayerEvent::Opened(info(true, true, tracks)));
        session.apply(&PlayerEvent::State(PlaybackState::Paused));
        session
    }

    #[test]
    fn transport_follows_the_state() {
        let mut session = Session::default();
        assert_eq!(session.transport(), None, "no file, no transport");
        session.begin();
        assert_eq!(session.transport(), Some(Transport::Play));
        session.apply(&PlayerEvent::Opened(info(true, true, &[])));
        session.apply(&PlayerEvent::State(PlaybackState::Playing));
        assert_eq!(session.transport(), Some(Transport::Pause));
        assert_eq!(session.play_label(), "Pause");
        session.apply(&PlayerEvent::State(PlaybackState::Ended));
        assert_eq!(session.transport(), Some(Transport::Replay));
        session.apply(&PlayerEvent::State(PlaybackState::Paused));
        assert_eq!(session.transport(), Some(Transport::Play));
        assert_eq!(session.play_label(), "Play");
    }

    #[test]
    fn seeking_is_clamped_to_the_file() {
        let mut session = opened(&[]);
        session.position = Duration::from_secs(3);
        assert_eq!(session.seek_target(-5.0), Duration::ZERO);
        assert_eq!(session.seek_target(5.0), Duration::from_secs(8));
        session.position = Duration::from_secs(58);
        assert_eq!(session.seek_target(5.0), Duration::from_secs(60));
        // Unknown duration: only the start is a limit.
        session.info.duration = None;
        assert_eq!(session.seek_target(5.0), Duration::from_secs(63));
    }

    #[test]
    fn volume_is_clamped_and_kept_across_files() {
        assert_eq!(Session::clamp_volume(f64::NAN), 0.0);
        assert_eq!(Session::clamp_volume(-3.0), 0.0);
        assert_eq!(Session::clamp_volume(140.0), 100.0);
        let mut session = Session::default();
        assert_eq!(session.volume, DEFAULT_VOLUME);
        session.set_volume(35.0);
        session.begin();
        assert_eq!(session.volume, 35.0);
    }

    #[test]
    fn subtitle_pop_up_maps_options_to_tracks() {
        let mut session = opened(&["English (1)", "French (2)"]);
        // Opening selects the first track.
        assert_eq!(session.subtitle, Some(0));
        let (items, selected) = session.subtitle_options();
        assert_eq!(items, ["Off", "English (1)", "French (2)"]);
        assert_eq!(selected, Some(1));
        assert!(session.choose_subtitle_option(2));
        assert_eq!(session.subtitle, Some(1));
        assert!(session.choose_subtitle_option(0));
        assert_eq!(session.subtitle, None);
        assert!(!session.choose_subtitle_option(3), "past the last track");
        assert_eq!(session.subtitle, None);
        session.toggle_subtitles();
        assert_eq!(session.subtitle, Some(0));
        session.toggle_subtitles();
        assert_eq!(session.subtitle, None);
    }

    #[test]
    fn files_without_text_streams_offer_nothing() {
        let mut session = opened(&[]);
        assert_eq!(session.subtitle, None);
        assert!(!session.has_subtitles());
        assert_eq!(
            session.subtitle_options(),
            (vec![NO_SUBTITLES.to_owned()], Some(0))
        );
        session.toggle_subtitles();
        assert_eq!(session.subtitle, None);
    }

    #[test]
    fn seeking_back_from_the_end_resumes_paused() {
        let mut session = opened(&[]);
        session.apply(&PlayerEvent::State(PlaybackState::Ended));
        assert_eq!(session.transport(), Some(Transport::Replay));
        // Seeking into the file leaves the end: Play, not Replay.
        session.seeked(Duration::from_secs(30));
        assert_eq!(session.position, Duration::from_secs(30));
        assert_eq!(session.state, PlaybackState::Paused);
        assert_eq!(session.transport(), Some(Transport::Play));
        // A seek to the very end keeps the finished state.
        session.apply(&PlayerEvent::State(PlaybackState::Ended));
        session.seeked(Duration::from_secs(60));
        assert_eq!(session.state, PlaybackState::Ended);
        // Playing stays playing across a seek.
        session.apply(&PlayerEvent::State(PlaybackState::Playing));
        session.seeked(Duration::from_secs(5));
        assert_eq!(session.state, PlaybackState::Playing);
    }

    #[test]
    fn a_session_keeps_its_choices_across_transport_and_seeks() {
        let mut session = opened(&["English (1)", "French (2)"]);
        session.apply(&PlayerEvent::State(PlaybackState::Playing));
        session.seeked(Duration::from_secs(12));
        assert_eq!(session.transport(), Some(Transport::Pause));
        session.apply(&PlayerEvent::State(PlaybackState::Paused));
        session.set_volume(35.0);
        assert!(session.choose_subtitle_option(2));
        session.seeked(Duration::from_secs(40));
        assert_eq!(session.position, Duration::from_secs(40));
        assert_eq!(session.volume, 35.0);
        assert_eq!(session.subtitle, Some(1));
        // A new file starts at the top, keeps the volume and turns on its
        // first track again.
        session.begin();
        assert_eq!(session.position, Duration::ZERO);
        assert_eq!(session.volume, 35.0);
        session.apply(&PlayerEvent::Opened(info(true, true, &["Dutch (1)"])));
        assert_eq!(session.subtitle, Some(0));
    }

    #[test]
    fn engine_failures_are_kept_and_drop_the_player() {
        let mut session = Session::default();
        session.begin();
        session.apply(&PlayerEvent::Unreadable("not a format".into()));
        let failure = session.failure.clone().unwrap();
        assert_eq!(failure.kind, FailureKind::Unreadable);
        assert!(!session.loaded && session.transport().is_none());

        session.begin();
        assert!(session.failure.is_none(), "a new file clears the failure");
        session.apply(&PlayerEvent::MissingCodec("No decoder. Install X.".into()));
        assert_eq!(
            session.failure.clone().unwrap().kind,
            FailureKind::MissingCodec
        );

        session.begin();
        session.apply(&PlayerEvent::Error("boom".into()));
        assert_eq!(session.failure.clone().unwrap().kind, FailureKind::Playback);
    }

    #[test]
    fn the_first_failure_is_kept() {
        let mut session = Session::default();
        session.begin();
        session.apply(&PlayerEvent::MissingCodec(
            "SLOPOS can't play: H.264".into(),
        ));
        session.apply(&PlayerEvent::Error("Your GStreamer installation".into()));
        let failure = session.failure.clone().unwrap();
        assert_eq!(failure.kind, FailureKind::MissingCodec);
        assert!(failure.message.contains("H.264"));
    }

    #[test]
    fn a_file_with_no_streams_fails_once_it_opens() {
        let mut session = Session::default();
        session.begin();
        session.apply(&PlayerEvent::Opened(info(false, false, &[])));
        assert_eq!(
            session.failure.clone().unwrap().kind,
            FailureKind::NoStreams
        );
        assert!(!session.loaded);
    }

    #[test]
    fn failure_titles_are_distinct() {
        let kinds = [
            FailureKind::Unavailable,
            FailureKind::Unreadable,
            FailureKind::MissingCodec,
            FailureKind::NoStreams,
            FailureKind::Playback,
        ];
        let titles: Vec<_> = kinds
            .iter()
            .map(|kind| Failure::new(*kind, "").title())
            .collect();
        for (index, title) in titles.iter().enumerate() {
            assert!(!titles[..index].contains(title), "duplicate {title}");
        }
    }

    #[test]
    fn unreadable_paths_say_why() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone.mp4");
        let failure = check_readable(&missing).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Unreadable);
        assert!(
            failure.message.contains("does not exist"),
            "{}",
            failure.message
        );

        let folder = check_readable(dir.path()).unwrap_err();
        assert!(folder.message.contains("is a folder"), "{}", folder.message);

        let file = dir.path().join("ok.mp4");
        std::fs::write(&file, b"x").unwrap();
        assert!(check_readable(&file).is_ok());

        assert!(io_message("a.mp4", io::ErrorKind::PermissionDenied).contains("isn't allowed"));
        assert!(io_message("a.mp4", io::ErrorKind::Other).contains("can't be read"));
    }

    #[test]
    fn status_line_describes_the_session() {
        let mut session = Session::default();
        session.begin();
        assert_eq!(session.status_line("a.webm"), "Opening “a.webm”…");
        session.apply(&PlayerEvent::Opened(info(true, true, &[])));
        session.apply(&PlayerEvent::State(PlaybackState::Paused));
        assert_eq!(
            session.status_line("a.webm"),
            "Paused — a.webm — 640 × 360 video with sound — 1:00"
        );
        session.apply(&PlayerEvent::State(PlaybackState::Ended));
        assert!(session.status_line("a.webm").starts_with("Finished"));
        session.apply(&PlayerEvent::Error("boom".into()));
        assert_eq!(session.status_line("a.webm"), "Playback stopped");
        assert_eq!(session.seek_value_text(), "0:00 of 1:00");
    }

    #[test]
    fn times_are_formatted() {
        assert_eq!(format_time(Duration::from_secs(5)), "0:05");
        assert_eq!(format_time(Duration::from_secs(125)), "2:05");
        assert_eq!(format_time(Duration::from_secs(3725)), "1:02:05");
    }

    #[test]
    fn file_names_fall_back_to_the_path() {
        assert_eq!(file_name(&PathBuf::from("/a/b/clip.mp4")), "clip.mp4");
        assert_eq!(file_name(&PathBuf::from("/")), "/");
    }
}
