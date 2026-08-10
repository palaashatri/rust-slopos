use crate::pty::Pty;
use crate::terminal::{PtyEvent, Terminal};
use nix::errno::Errno;
use nix::sys::signal::{self, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use slopos_kit::{
    AccessibilityNode, Event, EventResult, LayoutConstraint, Rect, Size, ThemeContext, Widget,
    WidgetState,
};
use slopos_sdk::EventLoopWaker;
use std::sync::{mpsc, Arc};

type WakeCallback = Arc<dyn Fn() + Send + Sync + 'static>;

fn no_op_wake() -> WakeCallback {
    Arc::new(|| {})
}

fn send_pty_event(tx: &mpsc::Sender<PtyEvent>, wake: &WakeCallback, event: PtyEvent) -> bool {
    if tx.send(event).is_ok() {
        wake();
        true
    } else {
        false
    }
}

fn pump_pty_reader<R>(mut read: R, tx: &mpsc::Sender<PtyEvent>, wake: &WakeCallback)
where
    R: FnMut(&mut [u8]) -> std::io::Result<usize>,
{
    let mut buf = [0u8; 1024];
    loop {
        match read(&mut buf) {
            Ok(n) if n > 0 => {
                if !send_pty_event(tx, wake, PtyEvent::Output(buf[..n].to_vec())) {
                    return;
                }
            }
            Ok(_) => {
                let _ = send_pty_event(tx, wake, PtyEvent::Exited);
                return;
            }
            Err(error) => {
                let _ = send_pty_event(tx, wake, PtyEvent::Error(error.to_string()));
                return;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChildShutdownResult {
    signal_requested: bool,
    reaped: bool,
    still_running: bool,
}

fn child_process_group(pid: Pid) -> Pid {
    match pid.as_raw().checked_neg() {
        Some(group_pid) if group_pid != 0 => Pid::from_raw(group_pid),
        _ => pid,
    }
}

fn best_effort_shutdown_child(pid: Pid) -> ChildShutdownResult {
    best_effort_shutdown_child_with(pid, signal::kill, |child| {
        waitpid(child, Some(WaitPidFlag::WNOHANG))
    })
}

fn best_effort_shutdown_child_with<K, W>(
    pid: Pid,
    mut send_signal: K,
    mut reap_child: W,
) -> ChildShutdownResult
where
    K: FnMut(Pid, Signal) -> Result<(), Errno>,
    W: FnMut(Pid) -> Result<WaitStatus, Errno>,
{
    fn observe_wait_status(
        pid: Pid,
        signal_requested: bool,
        status: Result<WaitStatus, Errno>,
    ) -> Option<ChildShutdownResult> {
        match status {
            Ok(WaitStatus::Exited(..) | WaitStatus::Signaled(..)) => Some(ChildShutdownResult {
                signal_requested,
                reaped: true,
                still_running: false,
            }),
            Ok(WaitStatus::StillAlive) => None,
            Ok(WaitStatus::Stopped(..) | WaitStatus::Continued(..)) => Some(ChildShutdownResult {
                signal_requested,
                reaped: false,
                still_running: true,
            }),
            // Linux exposes additional ptrace-only wait statuses. They do
            // not mean that the child has exited.
            #[cfg(target_os = "linux")]
            Ok(WaitStatus::PtraceEvent(..) | WaitStatus::PtraceSyscall(..)) => {
                Some(ChildShutdownResult {
                    signal_requested,
                    reaped: false,
                    still_running: true,
                })
            }
            Err(Errno::ECHILD | Errno::ESRCH) => Some(ChildShutdownResult {
                signal_requested,
                reaped: true,
                still_running: false,
            }),
            Err(err) => {
                tracing::debug!(child_pid = pid.as_raw(), %err, "terminal close: waitpid failed");
                Some(ChildShutdownResult {
                    signal_requested,
                    reaped: false,
                    still_running: true,
                })
            }
        }
    }

    let target = child_process_group(pid);
    let mut signal_requested = false;

    for signal_kind in [Signal::SIGHUP, Signal::SIGTERM] {
        match send_signal(target, signal_kind) {
            Ok(()) => {
                signal_requested = true;
                tracing::debug!(
                    child_pid = pid.as_raw(),
                    target_pid = target.as_raw(),
                    ?signal_kind,
                    "terminal close: requested child shutdown"
                );
            }
            Err(Errno::ESRCH) => {
                return ChildShutdownResult {
                    signal_requested,
                    reaped: true,
                    still_running: false,
                };
            }
            Err(err) => {
                tracing::debug!(
                    child_pid = pid.as_raw(),
                    target_pid = target.as_raw(),
                    ?signal_kind,
                    %err,
                    "terminal close: signal request failed"
                );
            }
        }

        if let Some(result) = observe_wait_status(pid, signal_requested, reap_child(pid)) {
            return result;
        }
    }

    observe_wait_status(pid, signal_requested, reap_child(pid)).unwrap_or(ChildShutdownResult {
        signal_requested,
        reaped: false,
        still_running: true,
    })
}

#[allow(dead_code)]
pub struct Tab {
    pub id: usize,
    pub title: String,
    pub term: Terminal,
    pub pty: Pty,
    pub child_pid: Pid,
}

#[allow(dead_code)]
impl Tab {
    pub fn pty(&self) -> &Pty {
        &self.pty
    }
    pub fn id(&self) -> usize {
        self.id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn child_pid(&self) -> Pid {
        self.child_pid
    }
}

pub struct TabManager {
    state: WidgetState,
    pub tabs: Vec<Tab>,
    pub active_tab_index: usize,
    next_tab_id: usize,
    wake_callback: WakeCallback,
}

impl TabManager {
    pub fn new() -> Self {
        TabManager {
            state: WidgetState::new(),
            tabs: vec![],
            active_tab_index: 0,
            next_tab_id: 1,
            wake_callback: no_op_wake(),
        }
    }

    /// Install the SDK event-loop wake path used by future PTY reader threads.
    ///
    /// The callback is intentionally injectable so isolated widget tests can
    /// count wake requests without constructing a display-backed SDK loop.
    pub fn set_wake_callback<F>(&mut self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.wake_callback = Arc::new(callback);
    }

    pub fn set_event_loop_waker(&mut self, waker: EventLoopWaker) {
        self.set_wake_callback(move || waker.wake());
    }

    pub fn open_tab(&mut self, cols: u16, rows: u16) -> Result<usize, String> {
        let (pty, pid) = Pty::new(cols, rows)?;
        let mut term = Terminal::new(cols as usize, rows as usize);

        let (tx, rx) = mpsc::channel::<PtyEvent>();
        let mut reader_pty = pty.try_clone().map_err(|e| e.to_string())?;
        let wake_callback = self.wake_callback.clone();

        std::thread::spawn(move || {
            pump_pty_reader(|buf| reader_pty.read(buf), &tx, &wake_callback);
        });

        term.pty = Some(pty.try_clone().map_err(|e| e.to_string())?);
        term.rx = Some(Arc::new(std::sync::Mutex::new(rx)));

        let id = self.next_tab_id;
        self.next_tab_id += 1;

        let title = format!("Shell {}", id);
        tracing::info!("Opening tab {} ({}) with PID {}", id, title, pid);

        let tab = Tab {
            id,
            title,
            term,
            pty,
            child_pid: pid,
        };
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        Ok(id)
    }

    #[allow(dead_code)]
    pub fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        let tab = &self.tabs[index];
        tracing::info!(
            "Closing tab {} ({}) with PID {}",
            tab.id,
            tab.title,
            tab.child_pid
        );
        let shutdown = best_effort_shutdown_child(tab.child_pid);
        tracing::info!(
            tab_id = tab.id,
            child_pid = tab.child_pid.as_raw(),
            signal_requested = shutdown.signal_requested,
            reaped = shutdown.reaped,
            still_running = shutdown.still_running,
            "Terminal tab close requested child shutdown"
        );
        self.tabs.remove(index);
        if self.active_tab_index >= self.tabs.len() && !self.tabs.is_empty() {
            self.active_tab_index = self.tabs.len() - 1;
        }
        true
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab_index)
    }

    pub fn active_tab_index(&self) -> usize {
        self.active_tab_index
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab_index)
    }

    #[allow(dead_code)]
    pub fn switch_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            true
        } else {
            false
        }
    }
}

impl Widget for TabManager {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));

        let rect = self.rect();
        if let Some(tab) = self.active_tab_mut() {
            let cols = (rect.width / 8.0).max(10.0) as usize;
            let rows = (rect.height / 16.0).max(5.0) as usize;
            tab.term.set_rect(rect);
            tab.term.resize_term(cols, rows);
            let _ = tab
                .term
                .layout(LayoutConstraint::tight(Size::new(rect.width, rect.height)));
        } else {
            return constraint.clamp(Size::ZERO);
        }
        size
    }

    fn draw(&self, theme: &ThemeContext) {
        if let Some(tab) = self.active_tab() {
            tab.term.draw(theme);
        }
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        if let Event::KeyDown { key, modifiers } = event {
            if modifiers.meta {
                match key {
                    slopos_kit::event::KeyCode::T => {
                        let _ = self.open_tab(80, 24);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::W if modifiers.shift => {
                        if !self.tabs.is_empty() {
                            let idx = self.active_tab_index;
                            self.close_tab(idx);
                        }
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key1 => {
                        self.switch_tab(0);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key2 => {
                        self.switch_tab(1);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key3 => {
                        self.switch_tab(2);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key4 => {
                        self.switch_tab(3);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key5 => {
                        self.switch_tab(4);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key6 => {
                        self.switch_tab(5);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key7 => {
                        self.switch_tab(6);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key8 => {
                        self.switch_tab(7);
                        return EventResult::Handled;
                    }
                    slopos_kit::event::KeyCode::Key9 => {
                        self.switch_tab(8);
                        return EventResult::Handled;
                    }
                    _ => {}
                }
            }
        }

        if let Some(tab) = self.active_tab_mut() {
            tab.term.handle_event(event)
        } else {
            EventResult::Ignored
        }
    }

    fn update(&mut self) {
        for tab in &mut self.tabs {
            tab.term.update();
            // Sync OSC title into tab title
            if tab.term.title_changed {
                if let Some(ref title) = tab.term.window_title.clone() {
                    if !title.is_empty() {
                        tab.title = title.clone();
                    }
                }
                tab.term.title_changed = false;
            }
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        None
    }

    fn children(&self) -> Vec<&dyn Widget> {
        if let Some(tab) = self.active_tab() {
            vec![&tab.term]
        } else {
            vec![]
        }
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        if let Some(tab) = self.active_tab_mut() {
            vec![&mut tab.term]
        } else {
            vec![]
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn child_shutdown_requests_sighup_and_reaps_immediately() {
        let pid = Pid::from_raw(42);
        let mut sent = Vec::new();
        let mut wait_calls = 0usize;

        let result = best_effort_shutdown_child_with(
            pid,
            |target, signal_kind| {
                sent.push((target.as_raw(), signal_kind));
                Ok(())
            },
            |_| {
                wait_calls += 1;
                Ok(WaitStatus::Exited(pid, 0))
            },
        );

        assert_eq!(
            sent,
            vec![(child_process_group(pid).as_raw(), Signal::SIGHUP)]
        );
        assert_eq!(wait_calls, 1);
        assert!(result.signal_requested);
        assert!(result.reaped);
        assert!(!result.still_running);
    }

    #[test]
    fn pty_reader_wakes_for_output_and_exit_without_polling() {
        let mut reader = io::Cursor::new(b"prompt".to_vec());
        let (tx, rx) = mpsc::channel();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let wake_count_for_callback = wake_count.clone();
        let wake: WakeCallback = Arc::new(move || {
            wake_count_for_callback.fetch_add(1, Ordering::SeqCst);
        });

        pump_pty_reader(|buf| reader.read(buf), &tx, &wake);

        assert_eq!(wake_count.load(Ordering::SeqCst), 2);
        assert_eq!(rx.recv().unwrap(), PtyEvent::Output(b"prompt".to_vec()));
        assert_eq!(rx.recv().unwrap(), PtyEvent::Exited);
    }

    #[test]
    fn pty_reader_wakes_for_read_error() {
        struct FailingReader;

        impl Read for FailingReader {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "reader failed"))
            }
        }

        let mut reader = FailingReader;
        let (tx, rx) = mpsc::channel();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let wake_count_for_callback = wake_count.clone();
        let wake: WakeCallback = Arc::new(move || {
            wake_count_for_callback.fetch_add(1, Ordering::SeqCst);
        });

        pump_pty_reader(|buf| reader.read(buf), &tx, &wake);

        assert_eq!(wake_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            rx.recv().unwrap(),
            PtyEvent::Error("reader failed".to_string())
        );
    }

    #[test]
    fn child_shutdown_escalates_to_sigterm_when_child_stays_alive() {
        let pid = Pid::from_raw(77);
        let mut sent = Vec::new();
        let mut wait_calls = 0usize;

        let result = best_effort_shutdown_child_with(
            pid,
            |target, signal_kind| {
                sent.push((target.as_raw(), signal_kind));
                Ok(())
            },
            |_| {
                wait_calls += 1;
                Ok(WaitStatus::StillAlive)
            },
        );

        assert_eq!(
            sent,
            vec![
                (child_process_group(pid).as_raw(), Signal::SIGHUP),
                (child_process_group(pid).as_raw(), Signal::SIGTERM),
            ]
        );
        assert_eq!(wait_calls, 3);
        assert!(result.signal_requested);
        assert!(!result.reaped);
        assert!(result.still_running);
    }

    #[test]
    fn child_shutdown_treats_missing_child_as_already_gone() {
        let pid = Pid::from_raw(99);
        let mut sent = Vec::new();
        let mut wait_calls = 0usize;

        let result = best_effort_shutdown_child_with(
            pid,
            |target, signal_kind| {
                sent.push((target.as_raw(), signal_kind));
                Ok(())
            },
            |_| {
                wait_calls += 1;
                Err(Errno::ECHILD)
            },
        );

        assert_eq!(
            sent,
            vec![(child_process_group(pid).as_raw(), Signal::SIGHUP)]
        );
        assert_eq!(wait_calls, 1);
        assert!(result.signal_requested);
        assert!(result.reaped);
        assert!(!result.still_running);
    }
}
