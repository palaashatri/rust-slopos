//! The Volume Mixer's controls: one vertical fader and a Mute check box per
//! channel, the status line above them, keyboard and pointer handling, and
//! the accessibility nodes. The Volume Mixer app and the top bar's popover
//! both host this, so they show the same thing and behave the same way.
//!
//! The view never talks to the sound server. A change the user makes is
//! returned as a [`Command`] for the host to run; the faders follow the
//! provider's next snapshot.

use accesskit::{Node, Role};
use slopos_services::audio::AudioSnapshot;
use slopos_ui::controls::range::{Checkbox, Slider, ToggleOutcome, ValueOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key, PointerButton};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

use crate::model::{self, Channel, Channels, Command, MAX_APPLICATIONS};

pub const MARGIN: i32 = 10;
const STATUS_HEIGHT: i32 = 18;
/// Width of one channel: fader, label and mute box.
pub const STRIP_WIDTH: u32 = 84;
const FADER_WIDTH: u32 = 28;
const PERCENT_HEIGHT: i32 = 16;
const FADER_HEIGHT: i32 = 150;
const GAP: i32 = 6;
const CHECK_HEIGHT: i32 = 18;
const NAME_HEIGHT: i32 = 16;
const MIN_WIDTH: u32 = 280;
/// The narrowest a channel may be drawn when the window is narrower than
/// all of them; below this, fewer channels are shown.
const MIN_STRIP_WIDTH: u32 = 56;
/// Faders run 0..=150 percent, like the sound server's own range.
const MAX_PERCENT: f64 = 150.0;

/// The window height. It does not depend on how many channels there are.
pub const HEIGHT: u32 = (MARGIN
    + STATUS_HEIGHT
    + PERCENT_HEIGHT
    + FADER_HEIGHT
    + GAP
    + CHECK_HEIGHT
    + GAP
    + NAME_HEIGHT
    + MARGIN) as u32;

/// Accessibility ids this view uses: the status line, then the fader and the
/// mute box of each channel. Hosts keep their own ids outside this range.
pub const NODE_BASE: u64 = 1_000;
pub const STATUS_NODE: u64 = NODE_BASE;

fn fader_node(index: usize) -> u64 {
    NODE_BASE + 1 + 2 * index as u64
}

fn mute_node(index: usize) -> u64 {
    NODE_BASE + 2 + 2 * index as u64
}

/// What the host should do after an event.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    /// Something on screen changed.
    pub redraw: bool,
    /// The user finished a change; the host sends it to the sound server.
    pub command: Option<Command>,
}

#[derive(Debug)]
struct Strip {
    channel: Channel,
    fader: Slider,
    mute: Checkbox,
    /// Where the percentage text goes.
    percent_at: Rect,
    /// Where the channel's name goes.
    name_at: Rect,
}

impl Strip {
    fn new(index: usize, channel: Channel, width: u32) -> Self {
        let mut fader = Slider::new(
            fader_node(index),
            &format!("{} volume", channel.name),
            0.0,
            MAX_PERCENT,
            1.0,
        )
        .vertical();
        fader.set_value(f64::from(channel.percent));
        let mute = Checkbox::new(mute_node(index), "Mute", channel.muted);
        let mut strip = Self {
            channel,
            fader,
            mute,
            percent_at: Rect::default(),
            name_at: Rect::default(),
        };
        strip.layout(index, width);
        strip
    }

    /// Place this channel's controls in column `index`, `width` pixels wide.
    fn layout(&mut self, index: usize, width: u32) {
        let x = MARGIN + index as i32 * width as i32;
        let fader_top = MARGIN + STATUS_HEIGHT + PERCENT_HEIGHT;
        self.fader.id = fader_node(index);
        self.fader.bounds = Rect {
            x: x + (width - FADER_WIDTH) as i32 / 2,
            y: fader_top,
            width: FADER_WIDTH,
            height: FADER_HEIGHT as u32,
        };
        let mute_top = fader_top + FADER_HEIGHT + GAP;
        self.mute.id = mute_node(index);
        self.mute.bounds = Rect {
            x: x + 6,
            y: mute_top,
            width: width - 12,
            height: CHECK_HEIGHT as u32,
        };
        let name_top = mute_top + CHECK_HEIGHT + GAP;
        self.percent_at = Rect {
            x,
            y: MARGIN + STATUS_HEIGHT,
            width,
            height: PERCENT_HEIGHT as u32,
        };
        self.name_at = Rect {
            x,
            y: name_top,
            width,
            height: NAME_HEIGHT as u32,
        };
    }
}

/// The mixer's controls and what they show.
#[derive(Debug)]
pub struct MixerView {
    strips: Vec<Strip>,
    /// Every channel the provider reported; `strips` shows as many as fit.
    channels: Vec<Channel>,
    /// What the provider said, before any "not shown" note.
    base_status: String,
    status: String,
    /// The window's width, if the host fixed one.
    width_limit: Option<u32>,
    font: Font,
    /// Position in the Tab order: `2 * channel` is the fader, `2 * channel + 1`
    /// the mute box.
    focus: Option<usize>,
}

impl Default for MixerView {
    fn default() -> Self {
        Self::new()
    }
}

impl MixerView {
    /// Before the first snapshot: no channels, and a status that says so.
    pub fn new() -> Self {
        Self {
            strips: Vec::new(),
            channels: Vec::new(),
            base_status: "Waiting for the sound server…".to_owned(),
            status: "Waiting for the sound server…".to_owned(),
            width_limit: None,
            font: Font::system(12.0),
            focus: None,
        }
    }

    /// Show the provider's latest answer. Faders being dragged keep their
    /// position and keyboard focus survives.
    pub fn set_state(&mut self, state: &Result<AudioSnapshot, String>) {
        match state {
            Ok(snapshot) => {
                let channels: Channels = model::channels(snapshot);
                self.base_status = model::status_line(&channels);
                self.channels = channels.channels;
            }
            Err(error) => {
                self.base_status = format!("Sound unavailable: {error}");
                self.channels = Vec::new();
            }
        }
        self.rebuild();
    }

    /// Fit the channels into a window `width` pixels wide. Channels narrow
    /// before any is dropped; when even the narrowest will not fit, the rest
    /// are left out and the status line says so.
    pub fn fit_width(&mut self, width: u32) {
        self.width_limit = Some(width);
        self.rebuild();
    }

    /// Whether accessibility request target `id` is one of this view's nodes.
    pub fn owns_node(&self, id: u64) -> bool {
        self.accessibility_children().contains(&id)
    }

    /// A command failed: say so, and keep the channels as they were.
    pub fn set_failure(&mut self, message: &str) {
        self.status = message.to_owned();
    }

    /// The status line, as shown.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// The channels in order, as shown.
    pub fn channels(&self) -> Vec<&Channel> {
        self.strips.iter().map(|strip| &strip.channel).collect()
    }

    /// The size this view wants: as wide as its channels need.
    pub fn size(&self) -> (u32, u32) {
        let strips = self.strips.len().max(1) as u32;
        let natural = (2 * MARGIN as u32 + strips * STRIP_WIDTH).max(MIN_WIDTH);
        let width = self.width_limit.map_or(natural, |limit| natural.min(limit));
        (width, HEIGHT)
    }

    /// The fixed window size for the app: room for every channel it can show.
    pub fn full_size() -> (u32, u32) {
        let strips = 2 + MAX_APPLICATIONS as u32;
        (2 * MARGIN as u32 + strips * STRIP_WIDTH, HEIGHT)
    }

    /// How many channels fit at the fitted width.
    fn capacity(&self) -> usize {
        match self.width_limit {
            None => 2 + MAX_APPLICATIONS,
            Some(limit) => {
                let room = limit.saturating_sub(2 * MARGIN as u32);
                (room / MIN_STRIP_WIDTH).max(1) as usize
            }
        }
    }

    /// The width of each of `shown` channels.
    fn strip_width(&self, shown: usize) -> u32 {
        match self.width_limit {
            None => STRIP_WIDTH,
            Some(limit) => {
                let room = limit.saturating_sub(2 * MARGIN as u32);
                (room / shown.max(1) as u32).clamp(MIN_STRIP_WIDTH, STRIP_WIDTH)
            }
        }
    }

    /// The flat focus position of the control with accessibility id `id`,
    /// if this view has one.
    fn position_of(&self, id: u64) -> Option<usize> {
        self.strips.iter().enumerate().find_map(|(index, strip)| {
            if strip.fader.id == id {
                Some(2 * index)
            } else if strip.mute.id == id {
                Some(2 * index + 1)
            } else {
                None
            }
        })
    }

    fn rebuild(&mut self) {
        let shown = self.channels.len().min(self.capacity());
        let hidden = self.channels.len() - shown;
        let width = self.strip_width(shown);
        let previous = std::mem::take(&mut self.strips);
        let focused = self.focus.and_then(|position| {
            previous
                .get(position / 2)
                .map(|strip| (strip.channel.target.clone(), position % 2))
        });
        self.strips = self.channels[..shown]
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, channel)| {
                let mut strip = Strip::new(index, channel, width);
                if let Some(old) = previous
                    .iter()
                    .find(|old| old.channel.target == strip.channel.target)
                {
                    // A drag in progress must carry on: keep the slider that is
                    // being dragged, moved to its new place.
                    if old.fader.dragging() {
                        let mut fader = old.fader.clone();
                        fader.id = strip.fader.id;
                        fader.bounds = strip.fader.bounds;
                        fader.label = strip.fader.label.clone();
                        strip.fader = fader;
                    }
                    // A Mute press in progress carries on in the same way; the
                    // box shows the provider's state.
                    let mut mute = old.mute.clone();
                    mute.checked = strip.mute.checked;
                    mute.id = strip.mute.id;
                    mute.bounds = strip.mute.bounds;
                    strip.mute = mute;
                }
                strip
            })
            .collect();
        self.focus = focused.and_then(|(target, part)| {
            self.strips
                .iter()
                .position(|strip| strip.channel.target == target)
                .map(|index| 2 * index + part)
        });
        self.status = if hidden > 0 {
            format!(
                "{} · {hidden} not shown here; see Control Panels",
                self.base_status
            )
        } else {
            self.base_status.clone()
        };
        self.sync_focus();
    }

    fn sync_focus(&mut self) {
        for (index, strip) in self.strips.iter_mut().enumerate() {
            strip.fader.focused = self.focus == Some(2 * index);
            strip.mute.focused = self.focus == Some(2 * index + 1);
        }
    }

    fn move_focus(&mut self, backwards: bool) {
        let count = 2 * self.strips.len();
        if count == 0 {
            self.focus = None;
            return;
        }
        self.focus = Some(match self.focus {
            None if backwards => count - 1,
            None => 0,
            Some(current) if backwards => (current + count - 1) % count,
            Some(current) => (current + 1) % count,
        });
        self.sync_focus();
    }

    /// Handle one event: keyboard focus, drags, clicks and accessibility
    /// actions. Tab and Shift+Tab move through the controls in order.
    pub fn handle(&mut self, event: &Event) -> Outcome {
        let mut outcome = Outcome::default();
        if let Event::Accessibility(AccessibilityRequest::Focus(id)) = event {
            if let Some(position) = self.position_of(*id) {
                self.focus = Some(position);
                self.sync_focus();
                outcome.redraw = true;
            }
            return outcome;
        }
        if let Event::KeyPressed {
            key: Key::Tab,
            modifiers,
        } = event
        {
            self.move_focus(modifiers.shift);
            outcome.redraw = true;
            return outcome;
        }
        // Pressing a Mute box gives it keyboard focus, as pressing a fader does.
        let mut focus_request = match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } => self
                .strips
                .iter()
                .position(|strip| strip.mute.bounds.contains(*x, *y))
                .map(|index| 2 * index + 1),
            _ => None,
        };
        for (index, strip) in self.strips.iter_mut().enumerate() {
            match strip.fader.handle(event) {
                ValueOutcome::Ignored => {}
                ValueOutcome::Changed => outcome.redraw = true,
                ValueOutcome::FocusRequested => {
                    focus_request = Some(2 * index);
                    outcome.redraw = true;
                }
                ValueOutcome::Value { value, last } => {
                    outcome.redraw = true;
                    // Only the release is a change to send; the drag is shown locally.
                    if last {
                        outcome.command = Some(Command::SetVolume {
                            target: strip.channel.target.clone(),
                            percent: value.round().max(0.0) as u32,
                        });
                    }
                }
            }
            match strip.mute.handle(event) {
                ToggleOutcome::Ignored => {}
                ToggleOutcome::Changed => outcome.redraw = true,
                ToggleOutcome::FocusRequested => {
                    focus_request = Some(2 * index + 1);
                    outcome.redraw = true;
                }
                ToggleOutcome::Toggled(muted) => {
                    outcome.redraw = true;
                    outcome.command = Some(Command::SetMuted {
                        target: strip.channel.target.clone(),
                        muted,
                    });
                }
            }
        }
        if let Some(position) = focus_request {
            self.focus = Some(position);
            self.sync_focus();
        }
        outcome
    }

    /// Paint the whole view at the origin of `surface`.
    pub fn paint(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        surface.clear(colors.surface)?;
        surface.draw_text(
            &self.status,
            MARGIN,
            MARGIN,
            &self.font,
            colors.ink_secondary,
        )?;
        for (index, strip) in self.strips.iter().enumerate() {
            if index > 0 {
                let x = strip.percent_at.x - 4;
                surface.fill_rect(
                    Rect {
                        x,
                        y: MARGIN + STATUS_HEIGHT,
                        width: 1,
                        height: HEIGHT - (2 * MARGIN + STATUS_HEIGHT) as u32,
                    },
                    colors.bevel_shadow,
                )?;
            }
            let percent = if strip.channel.muted {
                "Muted".to_owned()
            } else {
                format!("{}%", strip.channel.percent)
            };
            centred(surface, &percent, strip.percent_at, &self.font, colors.ink)?;
            strip.fader.paint(surface)?;
            strip.mute.paint(surface)?;
            let name = fit(&strip.channel.name, &self.font, strip.name_at.width - 8);
            centred(surface, &name, strip.name_at, &self.font, colors.ink)?;
        }
        Ok(())
    }

    /// The accessibility nodes: the status line, then each channel's fader
    /// and mute box. [`MixerView::accessibility_children`] lists the direct
    /// children of the window.
    pub fn accessibility_nodes(&self) -> Vec<(u64, Node)> {
        let mut status = Node::new(Role::Label);
        status.set_label(self.status.as_str());
        let mut nodes = vec![(STATUS_NODE, status)];
        for (index, strip) in self.strips.iter().enumerate() {
            nodes.push((fader_node(index), strip.fader.accessibility_node()));
            let mut mute = strip.mute.accessibility_node();
            mute.set_label(format!("Mute {}", strip.channel.name));
            nodes.push((mute_node(index), mute));
        }
        nodes
    }

    /// The accessibility node that has keyboard focus, if a control has it.
    pub fn focused_node(&self) -> Option<u64> {
        self.focus.map(|_| self.focus_node())
    }

    /// The node with keyboard focus; the status line when none has it.
    pub fn focus_node(&self) -> u64 {
        match self.focus {
            Some(position) if position % 2 == 0 => fader_node(position / 2),
            Some(position) => mute_node(position / 2),
            None => STATUS_NODE,
        }
    }

    pub fn accessibility_children(&self) -> Vec<u64> {
        let mut children = vec![STATUS_NODE];
        for index in 0..self.strips.len() {
            children.push(fader_node(index));
            children.push(mute_node(index));
        }
        children
    }
}

/// Draw `text` centred in `area`.
fn centred(
    surface: &RasterSurface,
    text: &str,
    area: Rect,
    font: &Font,
    color: slopos_ui::foundation::color::Color,
) -> Result<(), RenderError> {
    let width = measure(text, font).width as i32;
    let x = area.x + (area.width as i32 - width).max(0) / 2;
    surface.draw_text(text, x, area.y, font, color)
}

/// `text`, shortened with an ellipsis to fit `max_width`.
pub fn fit(text: &str, font: &Font, max_width: u32) -> String {
    if measure(text, font).width as u32 <= max_width {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while chars.pop().is_some() {
        let candidate: String = chars.iter().collect::<String>() + "…";
        if measure(&candidate, font).width as u32 <= max_width {
            return candidate;
        }
    }
    "…".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Target;
    use slopos_core::capability::Availability;
    use slopos_services::audio::{AudioDevice, AudioDirection, AudioStream};
    use slopos_ui::event::PointerButton;

    fn snapshot(streams: Vec<AudioStream>) -> AudioSnapshot {
        AudioSnapshot {
            revision: 1,
            availability: Availability::Available,
            default_output: Some("speakers".into()),
            default_input: None,
            outputs: vec![AudioDevice {
                index: 1,
                name: "speakers".into(),
                description: Some("Speakers".into()),
                direction: AudioDirection::Output,
                muted: false,
                volume_millipercent: vec![60_000, 60_000],
                is_default: true,
            }],
            inputs: Vec::new(),
            streams,
        }
    }

    fn stream(index: u32, application: &str, volume: u32) -> AudioStream {
        AudioStream {
            index,
            application: application.into(),
            media: None,
            output: 1,
            muted: false,
            volume_millipercent: vec![volume * 1000],
        }
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Default::default(),
        }
    }

    fn click_at(x: i32, y: i32) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        }
    }

    fn release_at(x: i32, y: i32) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
        }
    }

    /// The fader's bounds, for pointer tests.
    fn fader_bounds(view: &MixerView, index: usize) -> Rect {
        view.strips[index].fader.bounds
    }

    #[test]
    fn channels_follow_the_snapshot_and_the_status_says_what_it_is() {
        let mut view = MixerView::new();
        assert_eq!(view.status(), "Waiting for the sound server…");
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        let names: Vec<&str> = view.channels().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Master", "Music"]);
        assert_eq!(view.status(), "1 playing");
        // Two channels are narrower than the minimum width, so the window is MIN_WIDTH.
        assert_eq!(view.size().0, MIN_WIDTH);
        assert_eq!(view.size().1, HEIGHT);
    }

    #[test]
    fn a_missing_sound_server_is_reported_and_has_no_channels() {
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        view.set_state(&Err("no PulseAudio server".into()));
        assert!(view.channels().is_empty());
        assert_eq!(view.status(), "Sound unavailable: no PulseAudio server");
        assert!(view.accessibility_children() == [STATUS_NODE]);
    }

    #[test]
    fn a_failed_command_then_an_unreadable_server_drops_the_controls() {
        // The provider reports the failure first. When the server state cannot be
        // read either, the controls must not keep the value that was asked for.
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        view.set_failure("Could not change the sound: connection lost");
        assert_eq!(view.status(), "Could not change the sound: connection lost");
        assert_eq!(view.channels().len(), 2);
        view.set_state(&Err("connection lost".into()));
        assert!(view.channels().is_empty());
        assert_eq!(view.status(), "Sound unavailable: connection lost");
    }

    #[test]
    fn dragging_a_fader_sends_one_command_on_release() {
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        // The Music fader is the second channel; its track runs bottom to top.
        let bounds = fader_bounds(&view, 1);
        let top = bounds.y + 6;
        let bottom = bounds.y + bounds.height as i32 - 6;
        let x = bounds.x + bounds.width as i32 / 2;
        // The first press gives the fader keyboard focus only.
        let first = view.handle(&click_at(x, top));
        assert!(first.redraw);
        assert_eq!(first.command, None);
        // Drags move the fader on screen without sending anything yet.
        let moved = view.handle(&Event::PointerMoved { x, y: bottom });
        assert!(moved.redraw);
        assert_eq!(moved.command, None);
        let released = view.handle(&release_at(x, bottom));
        assert!(matches!(
            released.command,
            Some(Command::SetVolume {
                target: Target::Stream(7),
                percent: 0
            })
        ));
    }

    #[test]
    fn a_fader_drag_survives_a_snapshot_and_its_release_still_sends_a_command() {
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        let bounds = fader_bounds(&view, 1);
        let x = bounds.x + bounds.width as i32 / 2;
        let top = bounds.y + 6;
        let bottom = bounds.y + bounds.height as i32 - 6;
        view.handle(&click_at(x, top));
        view.handle(&Event::PointerMoved { x, y: bottom });
        // The provider reports the old volume while the user is still dragging:
        // the fader keeps where the user has taken it.
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        assert_eq!(view.strips[1].fader.value(), 0.0);
        let released = view.handle(&release_at(x, bottom));
        assert_eq!(
            released.command,
            Some(Command::SetVolume {
                target: Target::Stream(7),
                percent: 0
            })
        );
        // Once the drag is over, the provider's values are shown again.
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 10)])));
        assert_eq!(view.strips[1].fader.value(), 10.0);
    }

    #[test]
    fn a_mute_press_survives_a_snapshot_and_its_release_still_toggles() {
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        let mute = view.strips[1].mute.bounds;
        let (x, y) = (mute.x + 4, mute.y + 4);
        view.handle(&click_at(x, y));
        assert!(view.strips[1].mute.focused, "a press focuses the Mute box");
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        let released = view.handle(&release_at(x, y));
        assert_eq!(
            released.command,
            Some(Command::SetMuted {
                target: Target::Stream(7),
                muted: true
            })
        );
    }

    #[test]
    fn accessibility_focus_moves_to_the_named_control_and_its_keys_work() {
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        assert!(view.owns_node(mute_node(1)));
        assert!(!view.owns_node(12_345));
        let focused = view.handle(&Event::Accessibility(AccessibilityRequest::Focus(
            mute_node(1),
        )));
        assert!(focused.redraw);
        assert_eq!(view.focused_node(), Some(mute_node(1)));
        assert!(view.strips[1].mute.focused);
        assert!(!view.strips[0].mute.focused);
        let toggled = view.handle(&key(Key::Space));
        assert_eq!(
            toggled.command,
            Some(Command::SetMuted {
                target: Target::Stream(7),
                muted: true
            })
        );
    }

    #[test]
    fn a_narrow_window_fits_its_channels_and_says_what_it_leaves_out() {
        let mut view = MixerView::new();
        let streams = (0..MAX_APPLICATIONS as u32)
            .map(|i| stream(10 + i, &format!("App {i}"), 10))
            .collect();
        view.set_state(&Ok(snapshot(streams)));
        // 400 px leaves room for six columns of at least 56 px after the margins.
        view.fit_width(400);
        assert_eq!(view.strips.len(), 6);
        assert!(view
            .status()
            .ends_with("3 not shown here; see Control Panels"));
        assert!(view.size().0 <= 400);
        for strip in &view.strips {
            assert!(strip.percent_at.width >= MIN_STRIP_WIDTH);
            assert!(strip.percent_at.x >= 0);
            assert!(strip.percent_at.x + strip.percent_at.width as i32 <= 400 - MARGIN);
        }
        // A wide window shows them all again, at full column width.
        view.fit_width(MixerView::full_size().0);
        assert_eq!(view.strips.len(), 1 + MAX_APPLICATIONS);
        assert_eq!(view.strips[1].percent_at.width, STRIP_WIDTH);
        assert_eq!(view.status(), "8 playing");
    }

    #[test]
    fn the_mute_box_toggles_and_the_command_names_the_channel() {
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        let mute = view.strips[0].mute.bounds;
        let (x, y) = (mute.x + 4, mute.y + 4);
        view.handle(&click_at(x, y));
        let toggled = view.handle(&release_at(x, y));
        assert_eq!(
            toggled.command,
            Some(Command::SetMuted {
                target: Target::Output("speakers".into()),
                muted: true
            })
        );
    }

    #[test]
    fn tab_walks_the_controls_in_order_and_keys_drive_the_focused_fader() {
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        // Master fader, master mute, music fader, music mute, then wrap.
        view.handle(&key(Key::Tab));
        assert!(view.strips[0].fader.focused);
        view.handle(&key(Key::Tab));
        assert!(view.strips[0].mute.focused);
        view.handle(&key(Key::Tab));
        view.handle(&key(Key::Tab));
        assert!(view.strips[1].mute.focused);
        view.handle(&key(Key::Tab));
        assert!(view.strips[0].fader.focused);
        // Shift+Tab goes back.
        let back = Event::KeyPressed {
            key: Key::Tab,
            modifiers: slopos_ui::event::Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        view.handle(&back);
        assert!(view.strips[1].mute.focused);
        // Arrow keys move the focused fader by one step and send the change.
        for _ in 0..3 {
            view.handle(&key(Key::Tab));
        }
        assert!(view.strips[1].fader.focused);
        let up = view.handle(&key(Key::Up));
        assert_eq!(
            up.command,
            Some(Command::SetVolume {
                target: Target::Stream(7),
                percent: 41
            })
        );
    }

    #[test]
    fn accessibility_names_every_control_and_its_state() {
        let mut view = MixerView::new();
        view.set_state(&Ok(snapshot(vec![stream(7, "Music", 40)])));
        let nodes = view.accessibility_nodes();
        let ids: Vec<u64> = nodes.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            ids,
            [
                STATUS_NODE,
                fader_node(0),
                mute_node(0),
                fader_node(1),
                mute_node(1)
            ]
        );
        assert_eq!(view.accessibility_children(), ids);
        let (_, fader) = &nodes[3];
        assert_eq!(fader.label(), Some("Music volume"));
        assert_eq!(fader.role(), Role::Slider);
        let (_, mute) = &nodes[4];
        assert_eq!(mute.label(), Some("Mute Music"));
        assert_eq!(mute.role(), Role::CheckBox);
    }

    #[test]
    fn long_names_are_shortened_with_an_ellipsis() {
        let font = Font::system(12.0);
        let short = fit("Music", &font, 200);
        assert_eq!(short, "Music");
        let long = fit("An application with a very long name indeed", &font, 60);
        assert!(long.ends_with('…'));
        assert!(measure(&long, &font).width as u32 <= 60);
    }

    #[test]
    fn the_full_window_fits_the_maximum_number_of_channels() {
        let mut view = MixerView::new();
        let streams = (0..MAX_APPLICATIONS as u32 + 2)
            .map(|i| stream(10 + i, &format!("App {i}"), 10))
            .collect();
        view.set_state(&Ok(snapshot(streams)));
        // The master plus the applications that fit; the rest are only counted.
        assert_eq!(view.strips.len(), 1 + MAX_APPLICATIONS);
        assert!(view.size().0 <= MixerView::full_size().0);
        assert_eq!(
            MixerView::full_size().0,
            2 * MARGIN as u32 + 10 * STRIP_WIDTH
        );
    }
}
