//! The volume and brightness OSD: a small overlay that shows the new level
//! after a change, then hides. Its hide timer is armed only while the overlay is
//! visible, so an idle bar runs no timer for it.
//!
//! Volume changes come from the sound server's answers (its event stream, a
//! subscription). Brightness changes come from the brightness provider's watch,
//! which compares `/sys/class/backlight` snapshots on a 2-second timer inside
//! `slopos-services`; the bar itself runs no timer for it. When a provider
//! cannot say what the level is, the overlay says so instead of showing a made-up
//! level. Plain logic, tested without X.

use std::time::Duration;

use slopos_services::audio::AudioSnapshot;
use slopos_services::brightness::BrightnessSnapshot;

use crate::status::{self, State};

/// How long the overlay stays up after the latest change.
pub const VISIBLE_FOR: Duration = Duration::from_millis(1500);

/// Timer ids the OSD uses start here. They are below the toast timers (which
/// start at 10 000) and above the clock's id (1). A ring of ids means a timer
/// from an earlier change that fires late is recognised as stale.
pub const TIMER_BASE: u64 = 5_000;
const RING: u64 = 16;

/// Whether a timer id belongs to the OSD.
pub fn owns_timer(timer: u64) -> bool {
    (TIMER_BASE..TIMER_BASE + RING).contains(&timer)
}

/// What the overlay is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    Volume,
    Brightness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Level {
    /// The level in percent of the device's range (volume may exceed 100).
    Percent(u32),
    Muted,
    /// The provider cannot give a level; the text says why.
    Unavailable(String),
}

#[derive(Debug, Default)]
pub struct Osd {
    shown: Option<(Indicator, Level)>,
    generation: u64,
}

fn timer_for(generation: u64) -> u64 {
    TIMER_BASE + generation % RING
}

impl Osd {
    pub fn new() -> Self {
        Self::default()
    }

    /// A change to show. Returns the timer id to arm for `VISIBLE_FOR`; any
    /// earlier timer is now stale and will not hide the overlay.
    pub fn show(&mut self, what: Indicator, level: Level) -> u64 {
        self.generation += 1;
        self.shown = Some((what, level));
        timer_for(self.generation)
    }

    /// A timer the bar was asked to arm has fired. Returns true when the
    /// overlay hides now: it is visible and this is the timer for the latest
    /// change. Any other fire does nothing.
    pub fn timer_fired(&mut self, timer: u64) -> bool {
        if self.shown.is_some() && timer == timer_for(self.generation) {
            self.shown = None;
            true
        } else {
            false
        }
    }

    /// What is being shown and its level, if the overlay is visible.
    pub fn shown(&self) -> Option<(Indicator, &Level)> {
        self.shown.as_ref().map(|(what, level)| (*what, level))
    }
}

/// The overlay's line: "Volume 45%", "Brightness muted", or why there is no level.
pub fn text(what: Indicator, level: &Level) -> String {
    let name = match what {
        Indicator::Volume => "Volume",
        Indicator::Brightness => "Brightness",
    };
    match level {
        Level::Percent(percent) => format!("{name} {percent}%"),
        Level::Muted => format!("{name} muted"),
        Level::Unavailable(why) => format!("{name} unavailable: {why}"),
    }
}

/// How much of the bar to fill, from 0 to 1, or `None` when there is no level.
pub fn fraction(level: &Level) -> Option<f32> {
    match level {
        Level::Percent(percent) => Some((*percent as f32 / 100.0).clamp(0.0, 1.0)),
        Level::Muted | Level::Unavailable(_) => None,
    }
}

/// The level the overlay should show for a sound-server answer that follows
/// `previous`, or `None` when nothing should be shown. Only the default output's
/// volume or mute changing counts, and so does the server becoming unavailable
/// after a good answer. The first answer and a recovery are not changes to show.
pub fn volume_change(
    previous: &State<AudioSnapshot>,
    next: &State<AudioSnapshot>,
) -> Option<Level> {
    match (previous, next) {
        (Ok(before), Ok(after)) => {
            let level_of = |snapshot: &AudioSnapshot| {
                status::default_output(snapshot)
                    .map(|device| (status::volume_percent(device), device.muted))
            };
            let (now, was) = (level_of(after), level_of(before));
            match now {
                Some((percent, muted)) if now != was => Some(if muted {
                    Level::Muted
                } else {
                    Level::Percent(percent)
                }),
                _ => None,
            }
        }
        (Ok(_), Err(why)) => Some(Level::Unavailable(why.clone())),
        (Err(_), _) => None,
    }
}

/// The level the overlay should show for a brightness answer that follows
/// `previous`, or `None`. The first backlight device's level counts. A changed
/// level is shown; a backlight that was present and is now gone, or unreadable,
/// is shown as unavailable. A backlight appearing, the first answer and a
/// recovery are not changes to show, and neither is a machine with no backlight:
/// there is no level to change.
pub fn brightness_change(
    previous: &State<BrightnessSnapshot>,
    next: &State<BrightnessSnapshot>,
) -> Option<Level> {
    match (previous, next) {
        (Ok(before), Ok(after)) => {
            if before.devices.is_empty() {
                return None;
            }
            match after.devices.first() {
                None => Some(Level::Unavailable("no backlight device".into())),
                Some(device) => {
                    let was = before.devices.first().map(|d| d.percent());
                    (was != Some(device.percent()))
                        .then(|| Level::Percent(u32::from(device.percent())))
                }
            }
        }
        (Ok(before), Err(why)) if !before.devices.is_empty() => {
            Some(Level::Unavailable(why.clone()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_core::capability::Availability;
    use slopos_services::audio::{AudioDevice, AudioDirection};
    use slopos_services::brightness::BrightnessDevice;

    fn snapshot(volume: u32, muted: bool) -> AudioSnapshot {
        AudioSnapshot {
            revision: 1,
            availability: Availability::Available,
            default_output: Some("speakers".into()),
            default_input: None,
            outputs: vec![AudioDevice {
                index: 0,
                name: "speakers".into(),
                description: None,
                direction: AudioDirection::Output,
                muted,
                volume_millipercent: vec![volume * 1000, volume * 1000],
                is_default: true,
            }],
            inputs: Vec::new(),
            streams: Vec::new(),
        }
    }

    /// A backlight whose level is `percent` of a 0 to 1000 range.
    fn backlight(percent: u32) -> BrightnessSnapshot {
        BrightnessSnapshot {
            revision: 1,
            availability: Availability::Available,
            devices: vec![BrightnessDevice {
                name: "intel_backlight".into(),
                current: percent * 10,
                minimum: 0,
                maximum: 1000,
            }],
        }
    }

    fn no_backlight() -> BrightnessSnapshot {
        BrightnessSnapshot {
            revision: 1,
            availability: Availability::UnsupportedHardware,
            devices: Vec::new(),
        }
    }

    fn shown_text(osd: &Osd) -> String {
        let (what, level) = osd.shown().expect("something is shown");
        text(what, level)
    }

    #[test]
    fn nothing_is_visible_and_no_timer_is_armed_until_a_change() {
        let mut osd = Osd::new();
        assert!(osd.shown().is_none());
        // A timer that fires with the overlay hidden does nothing.
        assert!(!osd.timer_fired(TIMER_BASE));
        assert!(osd.shown().is_none());
    }

    #[test]
    fn a_change_shows_the_level_and_the_timer_hides_it_once() {
        let mut osd = Osd::new();
        let timer = osd.show(Indicator::Volume, Level::Percent(45));
        assert!(owns_timer(timer));
        assert_eq!(shown_text(&osd), "Volume 45%");
        assert!(osd.timer_fired(timer));
        assert!(osd.shown().is_none());
        // The same timer firing again does nothing: it is no longer armed.
        assert!(!osd.timer_fired(timer));
    }

    #[test]
    fn a_later_change_restarts_the_hide_and_an_earlier_timer_is_stale() {
        let mut osd = Osd::new();
        let first = osd.show(Indicator::Volume, Level::Percent(40));
        let second = osd.show(Indicator::Volume, Level::Percent(45));
        assert_ne!(first, second);
        // The earlier timer fires first but must not hide the newer level.
        assert!(!osd.timer_fired(first));
        assert_eq!(shown_text(&osd), "Volume 45%");
        assert!(osd.timer_fired(second));
        assert!(osd.shown().is_none());
    }

    #[test]
    fn the_ring_of_timer_ids_stays_in_the_osd_range() {
        let mut osd = Osd::new();
        for _ in 0..100 {
            let timer = osd.show(Indicator::Volume, Level::Percent(10));
            assert!(owns_timer(timer));
            assert!(timer < 10_000, "toast timers start at 10 000");
        }
        assert!(!owns_timer(1));
        assert!(!owns_timer(10_000));
    }

    #[test]
    fn an_unavailable_provider_is_said_in_words_and_has_no_bar() {
        let level = Level::Unavailable("no PulseAudio server".into());
        let mut osd = Osd::new();
        osd.show(Indicator::Volume, level.clone());
        assert_eq!(shown_text(&osd), "Volume unavailable: no PulseAudio server");
        assert_eq!(fraction(&level), None);
    }

    #[test]
    fn muted_has_text_but_no_level_bar() {
        assert_eq!(text(Indicator::Volume, &Level::Muted), "Volume muted");
        assert_eq!(fraction(&Level::Muted), None);
        assert_eq!(fraction(&Level::Percent(50)), Some(0.5));
        // Above 100 percent the bar is full, not overflowing.
        assert_eq!(fraction(&Level::Percent(130)), Some(1.0));
    }

    #[test]
    fn a_volume_key_change_shows_the_new_default_output_level() {
        let before = Ok(snapshot(40, false));
        assert_eq!(
            volume_change(&before, &Ok(snapshot(45, false))),
            Some(Level::Percent(45))
        );
        assert_eq!(
            volume_change(&before, &Ok(snapshot(40, true))),
            Some(Level::Muted)
        );
    }

    #[test]
    fn an_unchanged_answer_shows_nothing() {
        let before = Ok(snapshot(40, false));
        assert_eq!(volume_change(&before, &Ok(snapshot(40, false))), None);
    }

    #[test]
    fn the_first_answer_and_a_recovery_are_not_changes_to_show() {
        let loading: State<AudioSnapshot> = Err("loading".into());
        assert_eq!(volume_change(&loading, &Ok(snapshot(40, false))), None);
        assert_eq!(volume_change(&loading, &Err("no server".into())), None);
        let lost: State<AudioSnapshot> = Err("no server".into());
        assert_eq!(volume_change(&lost, &Ok(snapshot(30, false))), None);
    }

    #[test]
    fn losing_the_sound_server_after_a_good_answer_says_so() {
        let before = Ok(snapshot(40, false));
        assert_eq!(
            volume_change(&before, &Err("connection closed".into())),
            Some(Level::Unavailable("connection closed".into()))
        );
    }

    #[test]
    fn a_brightness_change_shows_the_new_backlight_level_and_only_that() {
        let before = Ok(backlight(40));
        assert_eq!(
            brightness_change(&before, &Ok(backlight(45))),
            Some(Level::Percent(45))
        );
        assert_eq!(
            text(Indicator::Brightness, &Level::Percent(45)),
            "Brightness 45%"
        );
        // The same level is not a change.
        assert_eq!(brightness_change(&before, &Ok(backlight(40))), None);
    }

    #[test]
    fn a_backlight_that_goes_away_or_cannot_be_read_is_said_to_be_unavailable() {
        let before = Ok(backlight(40));
        assert_eq!(
            brightness_change(&before, &Ok(no_backlight())),
            Some(Level::Unavailable("no backlight device".into()))
        );
        assert_eq!(
            brightness_change(&before, &Err("permission denied".into())),
            Some(Level::Unavailable("permission denied".into()))
        );
        assert_eq!(
            text(
                Indicator::Brightness,
                &Level::Unavailable("no backlight device".into())
            ),
            "Brightness unavailable: no backlight device"
        );
    }

    #[test]
    fn no_backlight_means_no_brightness_overlay() {
        // The first answer, a machine without a backlight, and a backlight that
        // appears are not changes to show: nothing is shown and no timer is armed.
        let loading: State<BrightnessSnapshot> = Err("loading".into());
        assert_eq!(brightness_change(&loading, &Ok(backlight(40))), None);
        assert_eq!(
            brightness_change(&Ok(no_backlight()), &Ok(no_backlight())),
            None
        );
        assert_eq!(
            brightness_change(&Ok(no_backlight()), &Ok(backlight(40))),
            None
        );
        assert_eq!(
            brightness_change(&Err("no sysfs".into()), &Ok(backlight(40))),
            None
        );
    }

    #[test]
    fn the_overlay_names_what_it_shows_and_a_later_change_replaces_it() {
        let mut osd = Osd::new();
        osd.show(Indicator::Volume, Level::Percent(40));
        osd.show(Indicator::Brightness, Level::Percent(70));
        assert_eq!(shown_text(&osd), "Brightness 70%");
        assert_eq!(
            osd.shown().map(|(what, _)| what),
            Some(Indicator::Brightness)
        );
    }
}
