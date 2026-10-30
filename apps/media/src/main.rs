//! SLOPOS Media: a native video and audio player on the SLOPOS toolkit.
//!
//! Playback runs in GStreamer (`engine.rs`); the window only shows the
//! newest frame and the controls. It sleeps until the engine wakes it with
//! a frame or event; while something is playing, a 250 ms timer keeps the
//! time display moving for audio-only files, and stops when playback stops.

mod engine;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use engine::{EngineError, MediaInfo, PlaybackState, Player, PlayerEvent};
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::file_chooser::{ChooserOutcome, FileChooser};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::range::{Checkbox, Slider, ToggleOutcome, ValueOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::{color::Color, geometry::Rect};
use slopos_ui::image::Image;
use slopos_ui::platform::{self, App, Control, Proxy, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

const WIDTH: u32 = 720;
const HEIGHT: u32 = 480;
const CONTROLS_HEIGHT: u32 = 64;
const TICK: u64 = 1;
const OPEN_ID: u64 = 10;
const PLAY_ID: u64 = 11;
const SEEK_ID: u64 = 12;
const MUTE_ID: u64 = 13;
const VOLUME_ID: u64 = 14;
const SUBTITLES_ID: u64 = 15;
const STATUS_ID: u64 = 2;
const VIDEO_ID: u64 = 3;
const CHOOSER_ID: u64 = 300;
const SCREEN: Color = Color {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 255,
};

/// Extensions offered in the Open dialog.
const EXTENSIONS: &[&str] = &[
    "webm", "mkv", "mp4", "m4v", "mov", "avi", "ogv", "ogg", "oga", "opus", "mp3", "flac", "wav",
    "m4a", "aac",
];

type WakeSlot = Arc<Mutex<Option<Proxy>>>;

fn format_time(time: Duration) -> String {
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

pub struct MediaApp {
    window: (u32, u32),
    slot: WakeSlot,
    player: Option<Player>,
    path: Option<PathBuf>,
    info: MediaInfo,
    state: PlaybackState,
    frame: Option<Image>,
    position: Duration,
    open: Button,
    play: Button,
    mute: Button,
    seek: Slider,
    volume: Slider,
    subtitles: Checkbox,
    focus: FocusChain,
    chooser: Option<FileChooser>,
    ticking: bool,
    status: String,
    font: Font,
}

impl MediaApp {
    pub fn new(path: Option<&Path>, slot: WakeSlot) -> Self {
        let mut volume = Slider::new(VOLUME_ID, "Volume", 0.0, 100.0, 5.0);
        volume.set_value(80.0);
        let mut app = Self {
            window: (WIDTH, HEIGHT),
            slot,
            player: None,
            path: None,
            info: MediaInfo::default(),
            state: PlaybackState::Stopped,
            frame: None,
            position: Duration::ZERO,
            open: Button::new(OPEN_ID, "Open…", ButtonPlateVariant::Regular),
            play: Button::new(PLAY_ID, "Play", ButtonPlateVariant::Default),
            mute: Button::new(MUTE_ID, "Mute", ButtonPlateVariant::Regular),
            seek: Slider::new(SEEK_ID, "Position", 0.0, 1.0, 1.0),
            volume,
            subtitles: Checkbox::new(SUBTITLES_ID, "Subtitles", true),
            focus: FocusChain::new(vec![
                OPEN_ID,
                PLAY_ID,
                SEEK_ID,
                MUTE_ID,
                VOLUME_ID,
                SUBTITLES_ID,
            ]),
            chooser: None,
            ticking: false,
            status: "Choose Open… to play a video or audio file.".into(),
            font: Font::system(12.0),
        };
        if let Err(error) = engine::init() {
            app.status = format!("Media playback is unavailable: {error}");
        }
        app.layout();
        app.refresh_controls();
        if let Some(path) = path {
            app.load(path);
        }
        app
    }

    fn video_area(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: self.window.0,
            height: self.window.1.saturating_sub(CONTROLS_HEIGHT),
        }
    }

    fn layout(&mut self) {
        let (width, height) = self.window;
        let top = height as i32 - CONTROLS_HEIGHT as i32;
        self.seek.bounds = Rect {
            x: 60,
            y: top + 8,
            width: width.saturating_sub(120),
            height: 18,
        };
        let row = top + 36;
        self.open.bounds = Rect {
            x: 8,
            y: row,
            width: 64,
            height: 20,
        };
        self.play.bounds = Rect {
            x: 82,
            y: row,
            width: 72,
            height: 20,
        };
        self.mute.bounds = Rect {
            x: width as i32 - 230,
            y: row,
            width: 56,
            height: 20,
        };
        self.volume.bounds = Rect {
            x: width as i32 - 166,
            y: row + 1,
            width: 100,
            height: 18,
        };
        self.subtitles = self.subtitles.clone().at(164, row + 1);
        if let Some(chooser) = &mut self.chooser {
            chooser.layout(self.window);
        }
    }

    fn refresh_controls(&mut self) {
        let loaded = self.player.is_some();
        self.play.enabled = loaded;
        self.play.label = if self.state == PlaybackState::Playing {
            "Pause"
        } else {
            "Play"
        }
        .into();
        self.seek.enabled = loaded && self.info.duration.is_some();
        self.mute.enabled = loaded && self.info.has_audio;
        self.volume.enabled = loaded && self.info.has_audio;
        self.subtitles.enabled = loaded && self.info.subtitles.is_some();
        self.mute.label = if self.player.as_ref().is_some_and(Player::muted) {
            "Unmute"
        } else {
            "Mute"
        }
        .into();
        if let Some(duration) = self.info.duration {
            self.seek.max = duration.as_secs_f64().max(0.001);
            self.seek.step = 1.0;
        }
        if !self.seek.dragging() {
            self.seek.set_value(self.position.as_secs_f64());
        }
    }

    fn sync_focus(&mut self) {
        let focused = self.focus.focused();
        self.open.focused = focused == Some(OPEN_ID);
        self.play.focused = focused == Some(PLAY_ID);
        self.seek.focused = focused == Some(SEEK_ID);
        self.mute.focused = focused == Some(MUTE_ID);
        self.volume.focused = focused == Some(VOLUME_ID);
        self.subtitles.focused = focused == Some(SUBTITLES_ID);
    }

    fn load(&mut self, path: &Path) {
        self.player = None;
        self.frame = None;
        self.info = MediaInfo::default();
        self.state = PlaybackState::Stopped;
        self.position = Duration::ZERO;
        let slot = self.slot.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Ok(guard) = slot.lock() {
                if let Some(proxy) = guard.as_ref() {
                    proxy.wake();
                }
            }
        });
        match Player::open(path, wake) {
            Ok(player) => {
                player.set_volume(self.volume.value() / 100.0);
                self.player = Some(player);
                self.path = Some(path.to_path_buf());
                self.status = format!("Opening “{}”…", file_name(path));
            }
            Err(EngineError::Unavailable(error)) => {
                self.status = format!("Media playback is unavailable: {error}")
            }
            Err(error) => self.status = format!("Can't open “{}”: {error}", file_name(path)),
        }
        self.refresh_controls();
    }

    fn toggle_play(&mut self, control: &mut Control) {
        let Some(player) = &self.player else { return };
        let result = match self.state {
            PlaybackState::Playing => player.pause(),
            PlaybackState::Ended => player.seek(Duration::ZERO).and_then(|()| player.play()),
            _ => player.play(),
        };
        if let Err(error) = result {
            self.status = error.to_string();
        }
        self.start_ticking(control);
    }

    fn start_ticking(&mut self, control: &mut Control) {
        if !self.ticking {
            self.ticking = true;
            control.start_timer(Duration::from_millis(250), TICK);
        }
    }

    fn seek_by(&mut self, seconds: f64) {
        let Some(player) = &self.player else { return };
        let target = (self.position.as_secs_f64() + seconds).max(0.0);
        let target = self
            .info
            .duration
            .map_or(target, |d| target.min(d.as_secs_f64()));
        if player.seek(Duration::from_secs_f64(target)).is_ok() {
            self.position = Duration::from_secs_f64(target);
        }
    }

    fn set_volume(&mut self, percent: f64) {
        self.volume.set_value(percent);
        if let Some(player) = &self.player {
            player.set_volume(self.volume.value() / 100.0);
        }
    }

    fn update_status(&mut self) {
        let Some(path) = &self.path else { return };
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
        self.status = format!("{state} — {} — {kind} — {}", file_name(path), duration);
    }

    /// Apply engine events and the newest frame. Returns whether to repaint.
    fn receive(&mut self, control: &mut Control) -> bool {
        let Some(player) = &self.player else {
            return false;
        };
        let mut changed = false;
        let mut failed = None;
        for event in player.poll_events() {
            changed = true;
            match event {
                PlayerEvent::Opened(info) => {
                    self.info = info;
                    if !self.info.has_video && !self.info.has_audio {
                        failed = Some("This file has no playable audio or video.".to_owned());
                    }
                }
                PlayerEvent::State(state) => self.state = state,
                PlayerEvent::Error(text) => failed = Some(format!("Playback failed: {text}")),
                PlayerEvent::MissingCodec(text) => failed = Some(text),
                PlayerEvent::Frame => {}
            }
        }
        if let Some(frame) = player.take_frame() {
            self.position = frame.position;
            if let Ok(image) = Image::from_rgba(frame.width, frame.height, frame.rgba.to_vec()) {
                self.frame = Some(image);
            }
            changed = true;
        }
        if let Some(position) = player.position() {
            self.position = position;
        }
        // Some containers only know their duration after a while.
        if self.info.duration.is_none() {
            self.info.duration = player.duration();
        }
        if let Some(text) = failed {
            self.player = None;
            self.state = PlaybackState::Stopped;
            self.status = text;
        } else if changed {
            self.update_status();
        }
        if self.state == PlaybackState::Playing {
            self.start_ticking(control);
        }
        self.refresh_controls();
        changed
    }

    fn handle(&mut self, event: &Event, control: &mut Control) {
        if let Some(chooser) = &mut self.chooser {
            match chooser.handle(event, control.modifiers()) {
                ChooserOutcome::Chosen(path) => {
                    self.chooser = None;
                    self.load(&path);
                }
                ChooserOutcome::Cancelled => self.chooser = None,
                ChooserOutcome::Copy(text) => control.set_clipboard(&text),
                ChooserOutcome::PasteRequested => control.request_paste(),
                _ => {}
            }
            return;
        }
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return;
        }
        // Keyboard shortcuts, unless a slider owns the arrow keys.
        if let Event::KeyPressed { key, modifiers } = event {
            let slider_focused = self.seek.focused || self.volume.focused;
            let button_focused = self.open.focused || self.play.focused || self.mute.focused;
            match key {
                Key::Character('o' | 'O') if modifiers.control => return self.open_chooser(),
                Key::Character('q' | 'Q' | 'w' | 'W') if modifiers.control => {
                    return control.quit()
                }
                Key::Space if !button_focused && !self.subtitles.focused => {
                    return self.toggle_play(control)
                }
                Key::Character('k' | 'K') => return self.toggle_play(control),
                Key::Left if !slider_focused => return self.seek_by(-5.0),
                Key::Right if !slider_focused => return self.seek_by(5.0),
                Key::Up if !slider_focused => return self.set_volume(self.volume.value() + 5.0),
                Key::Down if !slider_focused => return self.set_volume(self.volume.value() - 5.0),
                Key::Character('m' | 'M') => return self.toggle_mute(),
                _ => {}
            }
        }
        if let Event::Accessibility(AccessibilityRequest::Focus(id)) = event {
            self.focus.focus(*id);
            self.sync_focus();
            return;
        }
        let clicked = |id: u64| matches!(event, Event::Accessibility(AccessibilityRequest::Click(target)) if *target == id);
        if self.open.handle(event) == ButtonOutcome::Activated || clicked(OPEN_ID) {
            return self.open_chooser();
        }
        if self.play.handle(event) == ButtonOutcome::Activated || clicked(PLAY_ID) {
            return self.toggle_play(control);
        }
        if self.mute.handle(event) == ButtonOutcome::Activated || clicked(MUTE_ID) {
            return self.toggle_mute();
        }
        match self.seek.handle(event) {
            ValueOutcome::Value { value, last: true } => {
                if let Some(player) = &self.player {
                    if player.seek(Duration::from_secs_f64(value)).is_ok() {
                        self.position = Duration::from_secs_f64(value);
                    }
                }
            }
            ValueOutcome::FocusRequested => {
                self.focus.focus(SEEK_ID);
                self.sync_focus();
            }
            _ => {}
        }
        match self.volume.handle(event) {
            ValueOutcome::Value { value, .. } => self.set_volume(value),
            ValueOutcome::FocusRequested => {
                self.focus.focus(VOLUME_ID);
                self.sync_focus();
            }
            _ => {}
        }
        if let ToggleOutcome::Toggled(visible) = self.subtitles.handle(event) {
            if let Some(player) = &self.player {
                player.set_subtitles_visible(visible);
            }
        }
    }

    fn toggle_mute(&mut self) {
        if let Some(player) = &self.player {
            player.set_muted(!player.muted());
        }
        self.refresh_controls();
    }

    fn open_chooser(&mut self) {
        let dir = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        let filter = EXTENSIONS.iter().map(|ext| (*ext).to_owned()).collect();
        let mut chooser = FileChooser::new(CHOOSER_ID, "Open Media", &dir, Some(filter));
        chooser.layout(self.window);
        self.chooser = Some(chooser);
    }

    fn paint_all(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let (width, height) = self.window;
        surface.clear(colors.surface)?;
        let area = self.video_area();
        surface.fill_rect(area, SCREEN)?;
        if let Some(frame) = &self.frame {
            // Fit the frame inside the video area, keeping its aspect ratio.
            let scale = (f64::from(area.width) / f64::from(frame.width()))
                .min(f64::from(area.height) / f64::from(frame.height()));
            let w = (f64::from(frame.width()) * scale).round().max(1.0) as u32;
            let h = (f64::from(frame.height()) * scale).round().max(1.0) as u32;
            let x = area.x + (area.width as i32 - w as i32) / 2;
            let y = area.y + (area.height as i32 - h as i32) / 2;
            surface.with_clip(area, || surface.draw_image_scaled(frame, x, y, w, h))?;
        } else {
            let text = if self.player.is_some() && !self.info.has_video && self.info.has_audio {
                "♪"
            } else {
                ""
            };
            let big = Font::system(48.0);
            let tw = measure(text, &big).width as i32;
            surface.draw_text(
                text,
                (width as i32 - tw) / 2,
                area.height as i32 / 2 - 30,
                &big,
                Color::WHITE,
            )?;
        }
        // Status text over the bottom of the video area.
        let status_y = area.height as i32 - 22;
        surface.with_clip(
            Rect {
                x: 8,
                y: status_y,
                width: width.saturating_sub(16),
                height: 20,
            },
            || {
                surface.draw_text(&self.status, 9, status_y + 3, &self.font, SCREEN)?;
                surface.draw_text(&self.status, 8, status_y + 2, &self.font, Color::WHITE)
            },
        )?;
        let top = height as i32 - CONTROLS_HEIGHT as i32;
        surface.fill_rect(
            Rect {
                x: 0,
                y: top,
                width,
                height: 1,
            },
            colors.bevel_shadow,
        )?;
        let elapsed = format_time(self.position);
        let total = self.info.duration.map_or_else(|| "—".into(), format_time);
        surface.draw_text(&elapsed, 8, top + 9, &self.font, colors.ink)?;
        surface.draw_text(&total, width as i32 - 52, top + 9, &self.font, colors.ink)?;
        self.seek.paint(surface)?;
        self.open.paint(surface, &self.font)?;
        self.play.paint(surface, &self.font)?;
        self.mute.paint(surface, &self.font)?;
        self.volume.paint(surface)?;
        self.subtitles.paint(surface)?;
        if let Some(chooser) = &mut self.chooser {
            chooser.paint(surface)?;
        }
        Ok(())
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

impl App for MediaApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => return control.quit(),
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.layout();
            }
            Event::Wake => {
                if !self.receive(control) {
                    return;
                }
            }
            Event::Timer(TICK) => {
                self.ticking = false;
                self.receive(control);
                // Keep ticking only while playing; an idle player sleeps.
                if self.state == PlaybackState::Playing {
                    self.start_ticking(control);
                }
            }
            _ => {
                self.handle(event, control);
                self.refresh_controls();
            }
        }
        let title = self
            .path
            .as_deref()
            .map(file_name)
            .unwrap_or_else(|| "Media".into());
        control.set_title(&title);
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_all(surface) {
            eprintln!("slopos-media: paint failed: {error}");
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut nodes = vec![
            (OPEN_ID, self.open.accessibility_node()),
            (PLAY_ID, self.play.accessibility_node()),
            (SEEK_ID, {
                let mut node = self.seek.accessibility_node();
                node.set_value(format!(
                    "{} of {}",
                    format_time(self.position),
                    self.info
                        .duration
                        .map_or_else(|| "unknown".into(), format_time)
                ));
                node
            }),
            (MUTE_ID, self.mute.accessibility_node()),
            (VOLUME_ID, self.volume.accessibility_node()),
            (SUBTITLES_ID, self.subtitles.accessibility_node()),
        ];
        let mut video = accesskit::Node::new(accesskit::Role::Video);
        video.set_label(
            self.path
                .as_deref()
                .map(file_name)
                .unwrap_or_else(|| "No media".into()),
        );
        nodes.push((VIDEO_ID, video));
        let mut status = accesskit::Node::new(accesskit::Role::Label);
        status.set_label(self.status.as_str());
        nodes.push((STATUS_ID, status));
        let focus = match &self.chooser {
            Some(chooser) => {
                nodes.extend(chooser.accessibility_nodes());
                Some(chooser.focus_node())
            }
            None => self.focus.focused(),
        };
        Some(window_tree("Media", nodes, focus))
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let slot: WakeSlot = Arc::new(Mutex::new(None));
    let app = MediaApp::new(path.as_deref(), slot.clone());
    let options = WindowOptions::new("Media", "slopos-media", WIDTH, HEIGHT);
    if let Err(error) = platform::run(options, app, move |proxy| {
        if let Ok(mut guard) = slot.lock() {
            *guard = Some(proxy.clone());
        }
        proxy.wake();
    }) {
        eprintln!("slopos-media: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::event::Modifiers;
    use slopos_ui::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn pump(app: &mut MediaApp, until: impl Fn(&MediaApp) -> bool) {
        for _ in 0..1000 {
            let mut control = Control::default();
            app.event(&Event::Wake, &mut control);
            if until(app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("condition not reached; status: {}", app.status);
    }

    #[test]
    fn plays_a_video_with_keyboard_controls() {
        let dir = tempfile::tempdir().unwrap();
        let clip = engine::tests::video(dir.path());
        let mut app = MediaApp::new(Some(&clip), Arc::new(Mutex::new(None)));
        pump(&mut app, |app| app.info.has_video && app.frame.is_some());
        assert!(
            app.status.starts_with("Paused") && app.status.contains("160 × 120"),
            "{}",
            app.status
        );
        assert!(
            app.play.enabled && app.seek.enabled && app.volume.enabled && !app.subtitles.enabled
        );
        let mut control = Control::default();
        app.event(&key(Key::Down), &mut control);
        assert_eq!(app.volume.value(), 75.0);
        app.event(&key(Key::Character('m')), &mut control);
        assert!(app.player.as_ref().unwrap().muted());
        app.event(&key(Key::Space), &mut control);
        pump(&mut app, |app| app.state == PlaybackState::Playing);
        assert_eq!(app.play.label, "Pause");
        pump(&mut app, |app| app.state == PlaybackState::Ended);
        assert!(app.status.starts_with("Finished"));
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
        let tree = app.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .any(|(_, node)| node.role() == accesskit::Role::Slider));
    }

    #[test]
    fn unplayable_files_say_why() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("notes.mp4");
        std::fs::write(&text, b"this is text, not a video").unwrap();
        let mut app = MediaApp::new(Some(&text), Arc::new(Mutex::new(None)));
        pump(&mut app, |app| app.player.is_none());
        assert!(
            app.status.contains("not a video or audio format"),
            "{}",
            app.status
        );
        assert!(!app.play.enabled);
    }

    #[test]
    fn times_are_formatted() {
        assert_eq!(format_time(Duration::from_secs(5)), "0:05");
        assert_eq!(format_time(Duration::from_secs(125)), "2:05");
        assert_eq!(format_time(Duration::from_secs(3725)), "1:02:05");
    }
}
