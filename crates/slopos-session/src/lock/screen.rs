//! The lock surface: a full-screen window that covers the desktop, holds the
//! keyboard and pointer, shows the user's name and a password field, and
//! checks what is typed through PAM. Everything it draws and does is decided
//! by [`LockMachine`]; this module only connects it to the toolkit, the
//! password checker and logind.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use accesskit::TreeUpdate;
use slopos_ui::accessibility::{text_label, window_tree};
use slopos_ui::controls::text_field::TextField;
use slopos_ui::event::Event;
use slopos_ui::foundation::color::Color;
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{self, App, Control, WindowKind, WindowOptions};
use slopos_ui::render::RasterSurface;
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;
use zeroize::Zeroizing;

use super::auth::{run_checks, CheckRequest, PamCheck, PasswordCheck};
use super::clock::{Clock, SystemClock};
use super::logind::{self, EventSink, LogindEvent};
use super::state::{Effect, Input, LockMachine, Notice, Verdict};
use super::watch::signal_ready;

/// The timer that ends a failure delay.
const TIMER_DELAY: u64 = 1;
const TITLE_NODE: u64 = 10;
const FIELD_NODE: u64 = 11;
const NOTICE_NODE: u64 = 12;
const FIELD_CHARS: usize = 256;
const BACKDROP: Color = Color {
    red: 0x1E,
    green: 0x1E,
    blue: 0x1E,
    alpha: 255,
};
const PANEL_WIDTH: i32 = 360;
const PANEL_HEIGHT: i32 = 176;

pub struct LockApp {
    machine: LockMachine,
    clock: Box<dyn Clock>,
    user: String,
    field: TextField,
    notice: Notice,
    checks: Sender<CheckRequest>,
    verdicts: Receiver<Verdict>,
    logind: Receiver<LogindEvent>,
}

impl LockApp {
    /// The lock is active from the moment the app exists.
    pub fn new(
        user: String,
        clock: Box<dyn Clock>,
        checks: Sender<CheckRequest>,
        verdicts: Receiver<Verdict>,
        logind: Receiver<LogindEvent>,
    ) -> Self {
        let mut field = TextField::new(FIELD_NODE, "Password");
        field.secret = true;
        field.focused = true;
        field.max_chars = FIELD_CHARS;
        field.placeholder = "Password".to_owned();
        let mut machine = LockMachine::new();
        machine.handle(Input::Lock, clock.now());
        Self {
            machine,
            clock,
            user,
            field,
            notice: Notice::None,
            checks,
            verdicts,
            logind,
        }
    }

    /// Feed the machine one input and carry out what it asks for.
    fn feed(&mut self, input: Input, control: &mut Control) {
        let now = self.clock.now();
        let effects = self.machine.handle(input, now);
        self.apply(effects, control);
    }

    fn apply(&mut self, effects: Vec<Effect>, control: &mut Control) {
        for effect in effects {
            match effect {
                Effect::Show | Effect::Authenticate => {}
                Effect::Dismiss => {
                    self.field.clear();
                    control.quit();
                }
                Effect::ClearPassword => self.field.clear(),
                Effect::WakeAt(until) => {
                    let delay = until.saturating_sub(self.clock.now());
                    control.start_timer(delay, TIMER_DELAY);
                }
                Effect::ShowNotice(notice) => self.notice = notice,
            }
        }
        control.request_redraw();
    }

    /// Enter with a password typed starts a check. The password moves out of
    /// the field into a zeroing buffer that the checker drops after use.
    fn submit(&mut self, control: &mut Control) {
        if self.field.text().is_empty() {
            return;
        }
        let now = self.clock.now();
        let effects = self.machine.handle(Input::Submit, now);
        let checking = effects.contains(&Effect::Authenticate);
        let password = checking.then(|| Zeroizing::new(self.field.text().to_owned()));
        if checking {
            self.field.clear();
        }
        self.apply(effects, control);
        if let Some(password) = password {
            let request = CheckRequest {
                user: self.user.clone(),
                password,
            };
            if self.checks.send(request).is_err() {
                // The checker thread is gone. Stay locked and say why.
                self.feed(Input::Checked(Verdict::Unavailable), control);
            }
        }
    }

    /// Handle what the worker threads sent since the last wake-up.
    fn drain(&mut self, control: &mut Control) {
        while let Ok(event) = self.logind.try_recv() {
            // Lock and sleep signals change nothing while the screen is
            // already locked; only an unlock dismisses it.
            if event == LogindEvent::Unlock {
                self.feed(Input::Unlock, control);
            }
        }
        while let Ok(verdict) = self.verdicts.try_recv() {
            self.feed(Input::Checked(verdict), control);
        }
    }

    fn notice_text(&self) -> String {
        match self.notice {
            Notice::None => String::new(),
            Notice::Checking => "Checking…".to_owned(),
            Notice::Incorrect => "Incorrect password. Try again.".to_owned(),
            Notice::IncorrectWait(wait) => format!(
                "Incorrect password. Wait {} seconds before trying again.",
                wait.as_secs()
            ),
            Notice::Unavailable => "Unlocking is unavailable right now.".to_owned(),
        }
    }

    fn notice_is_error(&self) -> bool {
        matches!(
            self.notice,
            Notice::Incorrect | Notice::IncorrectWait(_) | Notice::Unavailable
        )
    }
}

impl App for LockApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::Wake => self.drain(control),
            Event::Timer(TIMER_DELAY) => self.feed(Input::Tick, control),
            Event::KeyPressed { .. }
            | Event::PointerPressed { .. }
            | Event::PointerReleased { .. }
            | Event::PointerMoved { .. }
            | Event::Paste(_) => {
                let outcome = self.field.handle(event);
                self.act_on_field(outcome, control);
            }
            // Resizes and focus changes repaint. Close requests, scrolling and
            // other input are ignored: nothing but the password unlocks.
            Event::Resized { .. } | Event::FocusChanged(_) => control.request_redraw(),
            _ => {}
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let colors = palette();
        let width = surface.logical_width() as i32;
        let height = surface.logical_height() as i32;
        let _ = surface.clear(BACKDROP);
        let panel = Rect {
            x: ((width - PANEL_WIDTH) / 2).max(0),
            y: ((height - PANEL_HEIGHT) / 2).max(0),
            width: PANEL_WIDTH.min(width).max(1) as u32,
            height: PANEL_HEIGHT.min(height).max(1) as u32,
        };
        let _ = surface.fill_rect(panel, colors.surface);
        let _ = surface.stroke_rect(panel, colors.bevel_shadow);

        let title_font = Font::system(16.0).bold();
        draw_centered(surface, &self.user, panel, 16, &title_font, colors.ink);
        let hint_font = Font::system(12.0);
        draw_centered(
            surface,
            "Type the password to unlock.",
            panel,
            44,
            &hint_font,
            colors.ink_secondary,
        );

        self.field.bounds = Rect {
            x: panel.x + 24,
            y: panel.y + 72,
            width: panel.width.saturating_sub(48),
            height: 24,
        };
        if let Err(error) = self.field.paint(surface) {
            log::warn!("cannot paint the password field: {error}");
        }

        let notice = self.notice_text();
        if !notice.is_empty() {
            let ink = if self.notice_is_error() {
                colors.danger
            } else {
                colors.ink
            };
            draw_centered(surface, &notice, panel, 116, &hint_font, ink);
        }
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let mut nodes = vec![
            (
                TITLE_NODE,
                text_label(format!(
                    "Screen locked. Type the password for {}.",
                    self.user
                )),
            ),
            (FIELD_NODE, self.field.accessibility_node()),
        ];
        let notice = self.notice_text();
        if !notice.is_empty() {
            nodes.push((NOTICE_NODE, text_label(notice)));
        }
        Some(window_tree("Screen locked", nodes, Some(FIELD_NODE)))
    }
}

impl LockApp {
    /// Enter is the only key that submits: the field reports it as
    /// `Submitted`, and that starts the check.
    fn act_on_field(
        &mut self,
        outcome: slopos_ui::controls::text_field::TextFieldOutcome,
        control: &mut Control,
    ) {
        use slopos_ui::controls::text_field::TextFieldOutcome as Outcome;
        match outcome {
            Outcome::Submitted => self.submit(control),
            Outcome::PasteRequested => control.request_paste(),
            Outcome::Changed | Outcome::Edited | Outcome::FocusRequested => {
                control.request_redraw()
            }
            // Escape cancels nothing on the lock; copy and cut are off for
            // secrets; other events do not concern the field.
            Outcome::Ignored | Outcome::Cancelled | Outcome::Copy(_) => {}
        }
    }
}

fn draw_centered(
    surface: &RasterSurface,
    text: &str,
    panel: Rect,
    top: i32,
    font: &Font,
    color: Color,
) {
    let width = measure(text, font).width.round() as i32;
    let x = panel.x + ((panel.width as i32 - width) / 2).max(8);
    if let Err(error) = surface.draw_text(text, x, panel.y + top, font, color) {
        log::warn!("cannot paint text: {error}");
    }
}

/// Lock the session now: run the locker window until it is unlocked.
/// `ready_fd` is the watcher's pipe; it is written once the screen is covered.
pub fn run(ready_fd: Option<i32>) -> Result<(), String> {
    let path = super::instance_lock_path()?;
    let instance =
        super::acquire_instance(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let Some(_instance) = instance else {
        log::info!("the screen is already locked");
        if let Some(fd) = ready_fd {
            signal_ready(fd);
        }
        return Ok(());
    };
    let user = super::current_user().ok_or("cannot find the name of the user")?;
    let checker: Box<dyn PasswordCheck> = Box::new(PamCheck::new());
    let (screen_width, screen_height) =
        slopos_ui::platform::x11::screen_size().map_err(|error| error.to_string())?;
    let factor = platform::scale_from_environment().factor();
    let mut options = WindowOptions::new(
        "Screen locked",
        "slopos-lock",
        (screen_width / factor).max(1),
        (screen_height / factor).max(1),
    );
    options.kind = WindowKind::Lock;
    options.resizable = false;
    options.position = Some((0, 0));

    let (check_sender, check_requests) = mpsc::channel::<CheckRequest>();
    let (verdict_sender, verdicts) = mpsc::channel::<Verdict>();
    let (logind_sender, logind_events) = mpsc::channel::<LogindEvent>();
    let app = LockApp::new(
        user,
        Box::new(SystemClock::new()),
        check_sender,
        verdicts,
        logind_events,
    );

    // logind's Unlock is the only signal the locker needs. Without a bus or
    // a session the password still unlocks, so this is best effort.
    let logind_session = zbus::blocking::Connection::system()
        .map_err(|error| error.to_string())
        .and_then(|bus| {
            logind::session_path(&bus)
                .map(|path| (bus, path))
                .map_err(|error| error.to_string())
        });
    let logind_session = match logind_session {
        Ok(session) => Some(session),
        Err(error) => {
            log::warn!("logind unlock is not watched: {error}");
            None
        }
    };

    let mut checker = Some(checker);
    let mut check_requests = Some(check_requests);
    let mut verdict_sender = Some(verdict_sender);
    let mut logind_sender = Some(logind_sender);
    let mut logind_session = logind_session;
    let mut ready_fd = ready_fd;
    platform::run(options, app, move |proxy| {
        // The checker thread blocks until a password arrives.
        if let (Some(checker), Some(requests), Some(verdicts_out)) =
            (checker.take(), check_requests.take(), verdict_sender.take())
        {
            let wake = proxy.clone();
            let spawned = thread::Builder::new()
                .name("slopos-lock-check".into())
                .spawn(move || {
                    run_checks(checker, requests, move |verdict| {
                        let _ = verdicts_out.send(verdict);
                        wake.wake();
                    });
                });
            if let Err(error) = spawned {
                log::error!("cannot start the password check: {error}");
            }
        }
        if let (Some((bus, path)), Some(events)) = (logind_session.take(), logind_sender.take()) {
            let wake = proxy.clone();
            let sink: EventSink = Arc::new(move |event| {
                let _ = events.send(event);
                wake.wake();
            });
            if let Err(error) = logind::watch_session(&bus, path.as_str(), sink) {
                log::warn!("cannot watch logind unlock: {error}");
            }
            // The subscription threads keep the connection alive.
            drop(bus);
        }
        // The window is up and holds input: tell the watcher to suspend.
        if let Some(fd) = ready_fd.take() {
            signal_ready(fd);
        }
    })
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::clock::ManualClock;
    use slopos_ui::event::{Key, Modifiers};
    use slopos_ui::foundation::geometry::Scale;
    use std::sync::Arc;
    use std::time::Duration;

    struct Harness {
        app: LockApp,
        clock: Arc<ManualClock>,
        checks: Receiver<CheckRequest>,
        verdicts: Sender<Verdict>,
        logind: Sender<LogindEvent>,
    }

    fn harness() -> Harness {
        let clock = Arc::new(ManualClock::default());
        let (check_sender, checks) = mpsc::channel();
        let (verdict_sender, verdicts) = mpsc::channel();
        let (logind_sender, logind_events) = mpsc::channel();
        let app = LockApp::new(
            "alice".into(),
            Box::new(clock.clone()),
            check_sender,
            verdicts,
            logind_events,
        );
        Harness {
            app,
            clock,
            checks,
            verdicts: verdict_sender,
            logind: logind_sender,
        }
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn type_text(app: &mut LockApp, control: &mut Control, text: &str) {
        for character in text.chars() {
            app.event(&key(Key::Character(character)), control);
        }
    }

    fn enter(app: &mut LockApp, control: &mut Control) {
        app.event(&key(Key::Enter), control);
    }

    /// Enter, then play the checker: answer `verdict` and deliver the wake-up.
    fn attempt(h: &mut Harness, control: &mut Control, password: &str, verdict: Verdict) {
        type_text(&mut h.app, control, password);
        enter(&mut h.app, control);
        let request = h.checks.try_recv().expect("a check was started");
        assert_eq!(&*request.password, password);
        h.verdicts.send(verdict).unwrap();
        h.app.event(&Event::Wake, control);
    }

    #[test]
    fn typing_and_enter_sends_one_check_and_empties_the_field() {
        let mut h = harness();
        let mut control = Control::default();
        type_text(&mut h.app, &mut control, "hunter2");
        assert_eq!(h.app.field.text(), "hunter2");
        enter(&mut h.app, &mut control);
        let request = h.checks.try_recv().expect("one check");
        assert_eq!(request.user, "alice");
        assert_eq!(&*request.password, "hunter2");
        assert!(h.checks.try_recv().is_err());
        assert_eq!(h.app.field.text(), "");
        assert_eq!(h.app.machine.phase(), crate::lock::state::Phase::Checking);
    }

    #[test]
    fn enter_on_an_empty_field_starts_nothing() {
        let mut h = harness();
        let mut control = Control::default();
        enter(&mut h.app, &mut control);
        assert!(h.checks.try_recv().is_err());
        assert_eq!(h.app.machine.phase(), crate::lock::state::Phase::Locked);
    }

    #[test]
    fn the_right_password_unlocks_and_quits() {
        let mut h = harness();
        let mut control = Control::default();
        attempt(&mut h, &mut control, "open sesame", Verdict::Accepted);
        assert!(control.quit_requested());
    }

    #[test]
    fn a_wrong_password_keeps_the_screen_locked_and_says_so() {
        let mut h = harness();
        let mut control = Control::default();
        attempt(&mut h, &mut control, "wrong", Verdict::Rejected);
        assert!(!control.quit_requested());
        assert_eq!(h.app.notice, Notice::Incorrect);
        assert!(h.app.notice_is_error());
        assert_eq!(h.app.field.text(), "");
    }

    #[test]
    fn escape_does_nothing_to_the_lock() {
        let mut h = harness();
        let mut control = Control::default();
        type_text(&mut h.app, &mut control, "abc");
        h.app.event(&key(Key::Escape), &mut control);
        assert!(!control.quit_requested());
        assert_eq!(h.app.field.text(), "abc");
        assert!(h.checks.try_recv().is_err());
    }

    #[test]
    fn close_requests_and_scrolling_do_not_unlock() {
        let mut h = harness();
        let mut control = Control::default();
        h.app.event(&Event::CloseRequested, &mut control);
        h.app.event(&Event::Scroll { dx: 0, dy: 3 }, &mut control);
        assert!(!control.quit_requested());
        assert_eq!(h.app.machine.phase(), crate::lock::state::Phase::Locked);
    }

    #[test]
    fn three_wrong_passwords_pause_checks_until_the_delay_ends() {
        let mut h = harness();
        let mut control = Control::default();
        for _ in 0..3 {
            attempt(&mut h, &mut control, "wrong", Verdict::Rejected);
        }
        assert_eq!(h.app.notice, Notice::IncorrectWait(Duration::from_secs(1)));
        // Typing during the pause starts no check.
        type_text(&mut h.app, &mut control, "right");
        enter(&mut h.app, &mut control);
        assert!(h.checks.try_recv().is_err());
        // The delay's timer fires: checks are accepted again.
        h.clock.advance(Duration::from_secs(1));
        h.app.event(&Event::Timer(TIMER_DELAY), &mut control);
        assert_eq!(h.app.notice, Notice::None);
        enter(&mut h.app, &mut control);
        assert!(h.checks.try_recv().is_ok());
    }

    #[test]
    fn a_timer_that_fires_early_keeps_the_pause() {
        let mut h = harness();
        let mut control = Control::default();
        for _ in 0..3 {
            attempt(&mut h, &mut control, "wrong", Verdict::Rejected);
        }
        h.clock.advance(Duration::from_millis(500));
        h.app.event(&Event::Timer(TIMER_DELAY), &mut control);
        assert!(matches!(
            h.app.machine.phase(),
            crate::lock::state::Phase::Delayed { .. }
        ));
    }

    #[test]
    fn a_logind_unlock_dismisses_the_lock() {
        let mut h = harness();
        let mut control = Control::default();
        h.logind.send(LogindEvent::Lock).unwrap();
        h.app.event(&Event::Wake, &mut control);
        assert!(!control.quit_requested(), "a lock request changes nothing");
        h.logind.send(LogindEvent::Unlock).unwrap();
        h.app.event(&Event::Wake, &mut control);
        assert!(control.quit_requested());
    }

    #[test]
    fn a_dead_checker_keeps_the_screen_locked() {
        let mut h = harness();
        let mut control = Control::default();
        // The checker thread is gone: nobody receives the request.
        drop(h.checks);
        type_text(&mut h.app, &mut control, "anything");
        enter(&mut h.app, &mut control);
        assert_eq!(h.app.notice, Notice::Unavailable);
        assert_eq!(h.app.machine.phase(), crate::lock::state::Phase::Locked);
        assert!(!control.quit_requested());
    }

    #[test]
    fn the_accessibility_tree_names_the_user_and_hides_the_password() {
        let mut h = harness();
        let mut control = Control::default();
        type_text(&mut h.app, &mut control, "secret");
        let tree = h.app.accessibility().expect("a tree");
        let (_, field) = tree
            .nodes
            .iter()
            .find(|(id, _)| id.0 == FIELD_NODE)
            .expect("the field node");
        assert_eq!(field.role(), accesskit::Role::PasswordInput);
        assert_eq!(field.value(), None);
        let (_, title) = tree
            .nodes
            .iter()
            .find(|(id, _)| id.0 == TITLE_NODE)
            .expect("the title node");
        assert!(title.value().unwrap().contains("alice"));
        assert_eq!(tree.focus.0, FIELD_NODE);
    }

    #[test]
    fn the_surface_paints_the_backdrop_and_the_panel() {
        let mut h = harness();
        let surface = RasterSurface::new(800, 600, Scale::new(1).unwrap()).unwrap();
        h.app.paint(&surface);
        let corner = surface.pixel(2, 2).expect("a pixel");
        let centre = surface.pixel(400, 300).expect("a pixel");
        assert_ne!(corner, centre, "the panel is not the backdrop");
        // The corner is the backdrop colour on the whole screen edge.
        assert_eq!(surface.pixel(799, 599), Some(corner));
    }
}
