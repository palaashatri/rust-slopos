//! SLOPOS Media: a native video and audio player on the SLOPOS toolkit.
//!
//! Playback runs in GStreamer (`engine.rs`) and its state rules are plain
//! logic in `session.rs`; the window only makes engine calls, shows the newest
//! frame and the controls, and explains failures in the video area. It sleeps
//! until the engine wakes it with a frame or event; while something is
//! playing, a 250 ms timer keeps the time display moving for audio-only
//! files, and stops when playback stops.

mod engine;
mod session;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use engine::{EngineError, PlaybackState, Player, PlayerEvent};
use session::{
    check_readable, file_name, Failure, FailureKind, Session, Transport, SEEK_STEP, VOLUME_STEP,
};
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::dialog::wrap;
use slopos_ui::controls::file_chooser::{ChooserOutcome, FileChooser};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem};
use slopos_ui::controls::popup::{PopUpButton, PopUpOutcome};
use slopos_ui::controls::range::{Slider, ValueOutcome};
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
const SUBTITLE_WIDTH: u32 = 150;
const TICK: u64 = 1;
const OPEN_ID: u64 = 10;
const PLAY_ID: u64 = 11;
const SEEK_ID: u64 = 12;
const MUTE_ID: u64 = 13;
const VOLUME_ID: u64 = 14;
const SUBTITLES_ID: u64 = 15;
const STATUS_ID: u64 = 2;
const VIDEO_ID: u64 = 3;
const ALERT_ID: u64 = 4;
const CHOOSER_ID: u64 = 300;
const MENU_ID: u64 = 100;

/// Playback menu items, by position.
const PLAY_ITEM: usize = 0;
const BACK_ITEM: usize = 2;
const FORWARD_ITEM: usize = 3;
const LOUDER_ITEM: usize = 5;
const QUIETER_ITEM: usize = 6;
const MUTE_ITEM: usize = 7;
const SUBTITLES_ITEM: usize = 9;

fn menu_bar() -> MenuBar {
    MenuBar::new(
        MENU_ID,
        vec![
            (
                "File",
                vec![
                    MenuItem::new("Open…").shortcut("Ctrl+O"),
                    MenuItem::separator(),
                    MenuItem::new("Close Window").shortcut("Ctrl+W"),
                ],
            ),
            (
                "Playback",
                vec![
                    MenuItem::new("Play").shortcut("Space"),
                    MenuItem::separator(),
                    MenuItem::new("Back 5 Seconds").shortcut("Left"),
                    MenuItem::new("Forward 5 Seconds").shortcut("Right"),
                    MenuItem::separator(),
                    MenuItem::new("Volume Up").shortcut("Up"),
                    MenuItem::new("Volume Down").shortcut("Down"),
                    MenuItem::new("Mute").shortcut("M").checkable(),
                    MenuItem::separator(),
                    MenuItem::new("Show Subtitles").checkable(),
                ],
            ),
        ],
    )
}
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

const NO_FILE_STATUS: &str = "Choose Open… to play a video or audio file.";

pub struct MediaApp {
    window: (u32, u32),
    slot: WakeSlot,
    player: Option<Player>,
    path: Option<PathBuf>,
    session: Session,
    frame: Option<Image>,
    open: Button,
    play: Button,
    mute: Button,
    seek: Slider,
    volume: Slider,
    subtitles: PopUpButton,
    focus: FocusChain,
    chooser: Option<FileChooser>,
    menu: MenuBar,
    ticking: bool,
    font: Font,
}

impl MediaApp {
    pub fn new(path: Option<&Path>, slot: WakeSlot) -> Self {
        let mut volume = Slider::new(VOLUME_ID, "Volume", 0.0, 100.0, 5.0);
        volume.set_value(session::DEFAULT_VOLUME);
        let mut subtitles = PopUpButton::new(SUBTITLES_ID, "Subtitles");
        subtitles.enabled = false;
        let mut app = Self {
            window: (WIDTH, HEIGHT),
            slot,
            player: None,
            path: None,
            session: Session::default(),
            frame: None,
            open: Button::new(OPEN_ID, "Open…", ButtonPlateVariant::Regular),
            play: Button::new(PLAY_ID, "Play", ButtonPlateVariant::Default),
            mute: Button::new(MUTE_ID, "Mute", ButtonPlateVariant::Regular),
            seek: Slider::new(SEEK_ID, "Position", 0.0, 1.0, 1.0),
            volume,
            subtitles,
            focus: FocusChain::new(vec![
                OPEN_ID,
                PLAY_ID,
                SEEK_ID,
                MUTE_ID,
                VOLUME_ID,
                SUBTITLES_ID,
            ]),
            chooser: None,
            menu: menu_bar(),
            ticking: false,
            font: Font::system(12.0),
        };
        if let Err(error) = engine::init() {
            app.session.fail(Failure::new(
                FailureKind::Unavailable,
                format!("{error}. Install GStreamer and its base plugins."),
            ));
        }
        app.layout();
        app.refresh_controls();
        if let Some(path) = path {
            app.load(path);
        }
        app
    }

    /// The bottom status line, as spoken and shown.
    fn status(&self) -> String {
        match &self.path {
            Some(path) => self.session.status_line(&file_name(path)),
            None => self
                .session
                .failure
                .as_ref()
                .map_or_else(|| NO_FILE_STATUS.to_owned(), |f| f.title().to_owned()),
        }
    }

    fn video_area(&self) -> Rect {
        let top = self.menu.height();
        Rect {
            x: 0,
            y: top as i32,
            width: self.window.0,
            height: self.window.1.saturating_sub(CONTROLS_HEIGHT + top),
        }
    }

    fn layout(&mut self) {
        let (width, height) = self.window;
        self.menu.bounds = Rect {
            x: 0,
            y: 0,
            width,
            height: self.menu.height(),
        };
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
        self.subtitles.bounds = Rect {
            x: 164,
            y: row,
            width: SUBTITLE_WIDTH,
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
        if let Some(chooser) = &mut self.chooser {
            chooser.layout(self.window);
        }
    }

    /// Bring every control and menu item in line with the session.
    fn refresh_controls(&mut self) {
        let loaded = self.session.loaded;
        let has_audio = self.session.info.has_audio;
        let muted = self.player.as_ref().is_some_and(Player::muted);

        self.play.enabled = loaded;
        self.play.label = self.session.play_label().into();
        self.seek.enabled = loaded && self.session.info.duration.is_some();
        self.mute.enabled = loaded && has_audio;
        self.volume.enabled = loaded && has_audio;
        self.mute.label = if muted { "Unmute" } else { "Mute" }.into();
        if let Some(duration) = self.session.info.duration {
            self.seek.max = duration.as_secs_f64().max(0.001);
            self.seek.step = 1.0;
        }
        if !self.seek.dragging() {
            self.seek.set_value(self.session.position.as_secs_f64());
        }
        if !self.volume.dragging() {
            self.volume.set_value(self.session.volume);
        }

        let (items, selected) = self.session.subtitle_options();
        self.subtitles.set_items(items, selected);
        self.subtitles.enabled = loaded && self.session.has_subtitles();

        let playback = &mut self.menu.menus[1].items;
        playback[PLAY_ITEM].label = self.session.play_label().into();
        playback[PLAY_ITEM].enabled = loaded;
        for item in [BACK_ITEM, FORWARD_ITEM] {
            playback[item].enabled = self.seek.enabled;
        }
        for item in [LOUDER_ITEM, QUIETER_ITEM, MUTE_ITEM] {
            playback[item].enabled = self.volume.enabled;
        }
        playback[MUTE_ITEM].checked = muted;
        playback[SUBTITLES_ITEM].enabled = self.subtitles.enabled;
        playback[SUBTITLES_ITEM].checked = self.session.subtitle.is_some();
    }

    fn menu_command(&mut self, menu: usize, item: usize, control: &mut Control) {
        match (menu, item) {
            (0, 0) => self.open_chooser(),
            (0, 2) => control.quit(),
            (1, PLAY_ITEM) => self.toggle_play(control),
            (1, BACK_ITEM) => self.seek_by(-SEEK_STEP),
            (1, FORWARD_ITEM) => self.seek_by(SEEK_STEP),
            (1, LOUDER_ITEM) => self.set_volume(self.session.volume + VOLUME_STEP),
            (1, QUIETER_ITEM) => self.set_volume(self.session.volume - VOLUME_STEP),
            (1, MUTE_ITEM) => self.toggle_mute(),
            (1, SUBTITLES_ITEM) => {
                self.session.toggle_subtitles();
                self.apply_subtitles();
            }
            _ => {}
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

    /// Stop showing the current file and explain why.
    fn fail(&mut self, failure: Failure) {
        self.player = None;
        self.frame = None;
        self.session.fail(failure);
        self.refresh_controls();
    }

    fn load(&mut self, path: &Path) {
        self.player = None;
        self.frame = None;
        self.path = Some(path.to_path_buf());
        self.session.begin();
        if let Err(failure) = check_readable(path) {
            return self.fail(failure);
        }
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
                player.set_volume(self.session.volume / 100.0);
                self.player = Some(player);
                self.refresh_controls();
            }
            Err(EngineError::Unavailable(error)) => self.fail(Failure::new(
                FailureKind::Unavailable,
                format!("{error}. Install GStreamer and its base plugins."),
            )),
            Err(error) => self.fail(Failure::new(
                FailureKind::Unreadable,
                format!("“{}” can't be played: {error}", file_name(path)),
            )),
        }
    }

    fn toggle_play(&mut self, control: &mut Control) {
        let Some(player) = &self.player else { return };
        let result = match self.session.transport() {
            Some(Transport::Pause) => player.pause(),
            Some(Transport::Replay) => player.seek(Duration::ZERO).and_then(|()| player.play()),
            Some(Transport::Play) => player.play(),
            None => return,
        };
        if let Err(error) = result {
            return self.fail(Failure::new(FailureKind::Playback, error.to_string()));
        }
        self.start_ticking(control);
        self.refresh_controls();
    }

    fn start_ticking(&mut self, control: &mut Control) {
        if !self.ticking {
            self.ticking = true;
            control.start_timer(Duration::from_millis(250), TICK);
        }
    }

    fn seek_by(&mut self, seconds: f64) {
        let target = self.session.seek_target(seconds);
        self.seek_to(target);
    }

    fn seek_to(&mut self, target: Duration) {
        let Some(player) = &self.player else { return };
        match player.seek(target) {
            Ok(()) => self.session.position = target,
            Err(error) => self.fail(Failure::new(FailureKind::Playback, error.to_string())),
        }
    }

    fn set_volume(&mut self, percent: f64) {
        self.session.set_volume(percent);
        if let Some(player) = &self.player {
            player.set_volume(self.session.volume / 100.0);
        }
        self.refresh_controls();
    }

    /// Push the session's subtitle choice to the engine.
    fn apply_subtitles(&mut self) {
        if let Some(player) = &self.player {
            player.set_subtitle_track(self.session.subtitle);
        }
        self.refresh_controls();
    }

    /// Apply engine events and the newest frame. Returns whether to repaint.
    fn receive(&mut self, control: &mut Control) -> bool {
        let Some(player) = &self.player else {
            return false;
        };
        let mut changed = false;
        let mut opened = false;
        for event in player.poll_events() {
            changed = true;
            opened |= matches!(event, PlayerEvent::Opened(_));
            self.session.apply(&event);
        }
        if let Some(frame) = player.take_frame() {
            self.session.position = frame.position;
            if let Ok(image) = Image::from_rgba(frame.width, frame.height, frame.rgba.to_vec()) {
                self.frame = Some(image);
            }
            changed = true;
        }
        if let Some(position) = player.position() {
            self.session.position = position;
        }
        // Some containers only know their duration after a while.
        if self.session.info.duration.is_none() {
            self.session.info.duration = player.duration();
        }
        if self.session.failure.is_some() {
            // The session recorded why; stop the engine and show the reason.
            self.player = None;
            self.frame = None;
            self.refresh_controls();
            return true;
        }
        if opened {
            self.apply_subtitles();
        }
        if self.session.state == PlaybackState::Playing {
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
        match self.menu.handle(event, self.window) {
            MenuBarOutcome::Chosen { menu, item } => return self.menu_command(menu, item, control),
            MenuBarOutcome::Changed => return,
            MenuBarOutcome::Ignored => {}
        }
        // An open subtitle pop-up takes every event.
        if self.subtitles.is_open() {
            if let PopUpOutcome::Selected(option) = self.subtitles.handle(event, self.window) {
                self.session.choose_subtitle_option(option);
                self.apply_subtitles();
            }
            return;
        }
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            return;
        }
        // Keyboard shortcuts, unless a control owns the key.
        if let Event::KeyPressed { key, modifiers } = event {
            let arrows_owned = self.seek.focused || self.volume.focused || self.subtitles.focused;
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
                Key::Left if !arrows_owned => return self.seek_by(-SEEK_STEP),
                Key::Right if !arrows_owned => return self.seek_by(SEEK_STEP),
                Key::Up if !arrows_owned => {
                    return self.set_volume(self.session.volume + VOLUME_STEP)
                }
                Key::Down if !arrows_owned => {
                    return self.set_volume(self.session.volume - VOLUME_STEP)
                }
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
                self.seek_to(Duration::from_secs_f64(value));
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
        match self.subtitles.handle(event, self.window) {
            PopUpOutcome::Selected(option) => {
                self.session.choose_subtitle_option(option);
                self.apply_subtitles();
            }
            PopUpOutcome::Changed => {
                self.focus.focus(SUBTITLES_ID);
                self.sync_focus();
            }
            PopUpOutcome::Ignored => {}
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

    /// The reason playback stopped, centred in the video area.
    fn paint_failure(
        &self,
        surface: &RasterSurface,
        area: Rect,
        failure: &Failure,
    ) -> Result<(), RenderError> {
        let title_font = Font::system(16.0);
        let body_font = Font::system(13.0);
        let width = area.width.saturating_sub(48).clamp(1, 520);
        let lines = wrap(&failure.message, &body_font, width);
        let line_height = measure("Ag", &body_font).height as i32;
        let title_height = measure("Ag", &title_font).height as i32;
        let total = title_height + 10 + line_height * lines.len() as i32;
        let centred = |text: &str, font: &Font| {
            let text_width = measure(text, font).width as i32;
            area.x + (area.width as i32 - text_width) / 2
        };
        let top = area.y + (area.height as i32 - total) / 2;
        let heading = Color::WHITE;
        let body = Color {
            red: 0xc8,
            green: 0xc8,
            blue: 0xc8,
            alpha: 0xff,
        };
        surface.draw_text(
            failure.title(),
            centred(failure.title(), &title_font),
            top,
            &title_font,
            heading,
        )?;
        for (row, line) in lines.iter().enumerate() {
            let y = top + title_height + 10 + row as i32 * line_height;
            surface.draw_text(line, centred(line, &body_font), y, &body_font, body)?;
        }
        Ok(())
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
        } else if let Some(failure) = &self.session.failure {
            self.paint_failure(surface, area, failure)?;
        } else {
            let text = if self.player.is_some() && !self.session.info.has_video {
                "♪"
            } else {
                ""
            };
            let big = Font::system(48.0);
            let tw = measure(text, &big).width as i32;
            surface.draw_text(
                text,
                (width as i32 - tw) / 2,
                area.y + area.height as i32 / 2 - 30,
                &big,
                Color::WHITE,
            )?;
        }
        // Status text across the top of the video area, clear of the bottom
        // edge where subtitles are drawn.
        let status = self.status();
        let status_y = area.y + 2;
        surface.with_clip(
            Rect {
                x: 8,
                y: status_y,
                width: width.saturating_sub(16),
                height: 20,
            },
            || {
                surface.draw_text(&status, 9, status_y + 3, &self.font, SCREEN)?;
                surface.draw_text(&status, 8, status_y + 2, &self.font, Color::WHITE)
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
        let elapsed = session::format_time(self.session.position);
        let total = self
            .session
            .info
            .duration
            .map_or_else(|| "—".into(), session::format_time);
        surface.draw_text(&elapsed, 8, top + 9, &self.font, colors.ink)?;
        surface.draw_text(&total, width as i32 - 52, top + 9, &self.font, colors.ink)?;
        self.seek.paint(surface)?;
        self.open.paint(surface, &self.font)?;
        self.play.paint(surface, &self.font)?;
        self.mute.paint(surface, &self.font)?;
        self.volume.paint(surface)?;
        self.subtitles.paint(surface, &self.font)?;
        self.menu.paint(surface)?;
        self.menu.paint_menus(surface)?;
        self.subtitles.paint_menu(surface)?;
        if let Some(chooser) = &mut self.chooser {
            chooser.paint(surface)?;
        }
        Ok(())
    }
}

impl App for MediaApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => return control.quit(),
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.layout();
            }
            // Applied even under the chooser: the bar's height changes the layout.
            Event::GlobalMenu(_) => {
                self.menu.handle(event, self.window);
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
                if self.session.state == PlaybackState::Playing {
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

    fn menu_bar(&self) -> Option<&MenuBar> {
        Some(&self.menu)
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut nodes = self.menu.accessibility_nodes();
        nodes.extend([
            (OPEN_ID, self.open.accessibility_node()),
            (PLAY_ID, self.play.accessibility_node()),
            (SEEK_ID, {
                let mut node = self.seek.accessibility_node();
                node.set_value(self.session.seek_value_text());
                node
            }),
            (MUTE_ID, self.mute.accessibility_node()),
            (VOLUME_ID, self.volume.accessibility_node()),
        ]);
        nodes.extend(self.subtitles.accessibility_nodes());
        let mut video = accesskit::Node::new(accesskit::Role::Video);
        video.set_label(
            self.path
                .as_deref()
                .map(file_name)
                .unwrap_or_else(|| "No media".into()),
        );
        nodes.push((VIDEO_ID, video));
        let mut status = slopos_ui::accessibility::text_label(self.status());
        status.set_live(accesskit::Live::Polite);
        nodes.push((STATUS_ID, status));
        if let Some(failure) = &self.session.failure {
            // A failure is announced at once and spoken in full.
            let mut alert = accesskit::Node::new(accesskit::Role::Alert);
            alert.set_label(format!("{}. {}", failure.title(), failure.message));
            alert.set_live(accesskit::Live::Assertive);
            nodes.push((ALERT_ID, alert));
        }
        let focus = match &self.chooser {
            Some(chooser) => {
                nodes.extend(chooser.accessibility_nodes());
                Some(chooser.focus_node())
            }
            None if self.subtitles.is_open() => Some(self.subtitles.focus_node()),
            None => self.menu.focus_node().or(self.focus.focused()),
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
        panic!("condition not reached; status: {}", app.status());
    }

    fn paint_at_both_scales(app: &mut MediaApp) {
        for factor in [1, 2] {
            let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(factor).unwrap()).unwrap();
            app.paint(&surface);
        }
    }

    #[test]
    fn plays_a_video_with_keyboard_controls() {
        let dir = tempfile::tempdir().unwrap();
        let clip = engine::tests::video(dir.path());
        let mut app = MediaApp::new(Some(&clip), Arc::new(Mutex::new(None)));
        pump(&mut app, |app| {
            app.session.info.has_video && app.frame.is_some()
        });
        assert!(
            app.status().starts_with("Paused") && app.status().contains("160 × 120"),
            "{}",
            app.status()
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
        pump(&mut app, |app| app.session.state == PlaybackState::Playing);
        assert_eq!(app.play.label, "Pause");
        pump(&mut app, |app| app.session.state == PlaybackState::Ended);
        assert!(app.status().starts_with("Finished"));
        paint_at_both_scales(&mut app);
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
        let failure = app.session.failure.clone().expect("a failure");
        assert_eq!(failure.kind, FailureKind::Unreadable);
        assert!(
            failure.message.contains("not a video or audio format"),
            "{}",
            failure.message
        );
        assert!(!app.play.enabled);
        // The reason is announced and drawn, not only in the status line.
        paint_at_both_scales(&mut app);
        let tree = app.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .any(|(id, node)| *id == accesskit::NodeId(ALERT_ID)
                && node.role() == accesskit::Role::Alert
                && node.live() == Some(accesskit::Live::Assertive)));
    }

    #[test]
    fn missing_files_are_reported_before_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let app = MediaApp::new(
            Some(&dir.path().join("gone.webm")),
            Arc::new(Mutex::new(None)),
        );
        assert!(app.player.is_none());
        let failure = app.session.failure.clone().expect("a failure");
        assert!(
            failure.message.contains("does not exist"),
            "{}",
            failure.message
        );
        assert_eq!(app.status(), "This file can't be played");
        assert!(app.open.enabled, "Open stays available after a failure");
    }

    #[test]
    fn subtitles_are_chosen_with_the_keyboard_and_the_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let clip = engine::tests::subtitled_mkv(dir.path());
        let mut app = MediaApp::new(Some(&clip), Arc::new(Mutex::new(None)));
        pump(&mut app, |app| {
            app.session.info.has_video && app.frame.is_some()
        });
        assert!(app.subtitles.enabled);
        assert_eq!(app.subtitles.items(), ["Off", "Subtitle (1)"]);
        assert_eq!(app.subtitles.selected(), Some(1), "on by default");
        assert!(app.player.as_ref().unwrap().subtitles_flag());

        // Keyboard: focus the pop-up, open it, move to Off and choose it.
        let mut control = Control::default();
        app.focus.focus(SUBTITLES_ID);
        app.sync_focus();
        app.event(&key(Key::Enter), &mut control);
        assert!(app.subtitles.is_open());
        app.event(&key(Key::Up), &mut control);
        app.event(&key(Key::Enter), &mut control);
        assert!(!app.subtitles.is_open());
        assert_eq!(app.session.subtitle, None);
        assert!(!app.player.as_ref().unwrap().subtitles_flag());
        assert_eq!(app.subtitles.value(), "Off");

        // Pointer-style accessibility click opens the pop-up; Escape closes it.
        app.event(
            &Event::Accessibility(AccessibilityRequest::Click(SUBTITLES_ID)),
            &mut control,
        );
        assert!(app.subtitles.is_open());
        app.event(&key(Key::Escape), &mut control);
        assert!(!app.subtitles.is_open());

        // The Playback menu's Show Subtitles turns the first track back on.
        app.event(
            &Event::MenuChosen {
                menu: 1,
                item: SUBTITLES_ITEM,
            },
            &mut control,
        );
        assert_eq!(app.session.subtitle, Some(0));
        assert!(app.player.as_ref().unwrap().subtitles_flag());

        // While the pop-up has focus, arrows open it and leave the volume alone.
        app.focus.focus(SUBTITLES_ID);
        app.sync_focus();
        let before = app.session.volume;
        app.event(&key(Key::Down), &mut control);
        assert!(app.subtitles.is_open());
        assert_eq!(app.session.volume, before);
        app.event(&key(Key::Escape), &mut control);

        paint_at_both_scales(&mut app);
        let tree = app.accessibility().unwrap();
        assert!(tree
            .nodes
            .iter()
            .any(|(id, node)| *id == accesskit::NodeId(SUBTITLES_ID)
                && node.role() == accesskit::Role::ComboBox
                && node.label() == Some("Subtitles")));
    }

    #[test]
    fn a_file_without_text_streams_has_a_disabled_subtitle_control() {
        let dir = tempfile::tempdir().unwrap();
        let clip = engine::tests::video(dir.path());
        let mut app = MediaApp::new(Some(&clip), Arc::new(Mutex::new(None)));
        pump(&mut app, |app| {
            app.session.info.has_video && app.frame.is_some()
        });
        assert!(!app.subtitles.enabled);
        assert_eq!(app.subtitles.value(), session::NO_SUBTITLES);
        let mut control = Control::default();
        app.focus.focus(SUBTITLES_ID);
        app.sync_focus();
        app.event(&key(Key::Enter), &mut control);
        assert!(!app.subtitles.is_open(), "a disabled control does not open");
    }

    #[test]
    fn playback_menu_follows_the_player_and_moves_to_the_global_bar() {
        let mut app = MediaApp::new(None, Arc::new(Mutex::new(None)));
        let mut control = Control::default();
        let playback = &app.menu.menus[1].items;
        // Nothing loaded: every playback command is disabled.
        assert!(!playback[PLAY_ITEM].enabled && !playback[MUTE_ITEM].enabled);
        assert!(playback[MUTE_ITEM].checkable && playback[SUBTITLES_ITEM].checkable);
        assert_eq!(
            app.menu.handle(
                &Event::MenuChosen {
                    menu: 1,
                    item: PLAY_ITEM
                },
                app.window
            ),
            MenuBarOutcome::Ignored
        );
        assert_eq!(app.video_area().y, 20);
        app.event(&Event::GlobalMenu(true), &mut control);
        assert_eq!(app.video_area().y, 0);
        assert_eq!(app.video_area().height, HEIGHT - CONTROLS_HEIGHT);
        // File > Open… from the global bar opens the chooser.
        app.event(&Event::MenuChosen { menu: 0, item: 0 }, &mut control);
        assert!(app.chooser.is_some());
        app.event(&key(Key::Escape), &mut control);
        app.event(&Event::MenuChosen { menu: 0, item: 2 }, &mut control);
        assert!(control.quit_requested());
    }
}
