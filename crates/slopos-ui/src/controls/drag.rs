//! Drag and drop: the pointer state machine behind a drag gesture.
//!
//! A control that can be dragged calls [`DragController::press`] on a primary
//! press with the payload it offers. A drag starts only when the pointer has
//! moved at least the threshold away from the press, so a plain click never
//! becomes a drag. While dragging, the controller reports each move (so the
//! app can draw feedback) and the release point; the app decides which drop
//! target the release point is over and whether the payload is accepted.
//! Escape, or losing keyboard focus, cancels.
//!
//! This module holds no platform code. Moving a payload between windows or
//! applications (XDND on X11) belongs to the platform adapter and is not
//! implemented here.

use crate::event::{Event, Key, PointerButton};

/// Most items one drag may carry. A larger payload is refused by
/// [`DragController::press`].
pub const MAX_DRAG_ITEMS: usize = 4096;

/// What a drag carries: a kind the target checks, and the source's item keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragPayload {
    /// The kind of data, such as "files" or "tree-items".
    pub kind: String,
    /// Opaque keys the source chose, such as row keys or file ids.
    pub items: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragOutcome {
    /// The event did not concern the drag.
    Ignored,
    /// The button is down but the pointer has not moved far enough yet.
    Pending,
    /// The pointer passed the threshold: the drag has started at `x, y`.
    Started { x: i32, y: i32 },
    /// The drag moved; draw the feedback at `x, y`.
    Moved { x: i32, y: i32 },
    /// The primary button was released during a drag, over `x, y`.
    Dropped {
        payload: DragPayload,
        x: i32,
        y: i32,
    },
    /// The drag was cancelled (Escape, or focus was lost).
    Cancelled,
    /// The button was released without a drag: an ordinary click.
    NotDragged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Idle,
    Pressed {
        start: (i32, i32),
        payload: DragPayload,
    },
    Dragging {
        payload: DragPayload,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragController {
    threshold: u32,
    state: State,
}

/// Whether the pointer has moved at least `threshold` pixels on either axis.
/// The arithmetic is done in 64 bits, so extreme coordinates cannot overflow.
fn moved_past(start: (i32, i32), now: (i32, i32), threshold: u32) -> bool {
    let dx = (i64::from(now.0) - i64::from(start.0)).abs();
    let dy = (i64::from(now.1) - i64::from(start.1)).abs();
    dx.max(dy) >= i64::from(threshold)
}

impl DragController {
    /// A controller that starts a drag after `threshold` logical pixels.
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold,
            state: State::Idle,
        }
    }

    /// True between a press and the release, whether or not the drag started.
    pub fn is_active(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    /// True once the drag has started.
    pub fn is_dragging(&self) -> bool {
        matches!(self.state, State::Dragging { .. })
    }

    /// Begin a possible drag at `x, y` with `payload`. Returns false, and
    /// stays idle, when the payload is empty or holds more than
    /// [`MAX_DRAG_ITEMS`] items.
    pub fn press(&mut self, x: i32, y: i32, payload: DragPayload) -> bool {
        if payload.items.is_empty() || payload.items.len() > MAX_DRAG_ITEMS {
            return false;
        }
        self.state = State::Pressed {
            start: (x, y),
            payload,
        };
        true
    }

    /// Handle an event; only pointer movement, primary release and cancel
    /// keys matter here.
    pub fn handle(&mut self, event: &Event) -> DragOutcome {
        match event {
            Event::PointerMoved { x, y } => match std::mem::replace(&mut self.state, State::Idle) {
                State::Idle => DragOutcome::Ignored,
                State::Pressed { start, payload } => {
                    if moved_past(start, (*x, *y), self.threshold) {
                        self.state = State::Dragging { payload };
                        DragOutcome::Started { x: *x, y: *y }
                    } else {
                        self.state = State::Pressed { start, payload };
                        DragOutcome::Pending
                    }
                }
                State::Dragging { payload } => {
                    self.state = State::Dragging { payload };
                    DragOutcome::Moved { x: *x, y: *y }
                }
            },
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            } => match std::mem::replace(&mut self.state, State::Idle) {
                State::Idle => DragOutcome::Ignored,
                State::Pressed { .. } => DragOutcome::NotDragged,
                State::Dragging { payload } => DragOutcome::Dropped {
                    payload,
                    x: *x,
                    y: *y,
                },
            },
            Event::KeyPressed {
                key: Key::Escape, ..
            }
            | Event::FocusChanged(false)
                if self.is_active() =>
            {
                self.state = State::Idle;
                DragOutcome::Cancelled
            }
            _ => DragOutcome::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;

    fn payload() -> DragPayload {
        DragPayload {
            kind: "files".to_owned(),
            items: vec![1, 2],
        }
    }

    fn moved(x: i32, y: i32) -> Event {
        Event::PointerMoved { x, y }
    }

    fn release(x: i32, y: i32) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
        }
    }

    #[test]
    fn a_click_without_movement_is_not_a_drag() {
        let mut drag = DragController::new(4);
        assert!(drag.press(5, 5, payload()));
        assert_eq!(drag.handle(&release(5, 5)), DragOutcome::NotDragged);
        assert!(!drag.is_active());
    }

    #[test]
    fn movement_below_the_threshold_stays_pending_then_starts() {
        let mut drag = DragController::new(4);
        drag.press(10, 10, payload());
        assert_eq!(drag.handle(&moved(12, 10)), DragOutcome::Pending);
        assert!(!drag.is_dragging());
        assert_eq!(
            drag.handle(&moved(10, 14)),
            DragOutcome::Started { x: 10, y: 14 }
        );
        assert!(drag.is_dragging());
    }

    #[test]
    fn a_drag_reports_moves_and_drops_its_payload_at_the_release_point() {
        let mut drag = DragController::new(0);
        drag.press(0, 0, payload());
        assert_eq!(
            drag.handle(&moved(20, 20)),
            DragOutcome::Started { x: 20, y: 20 }
        );
        assert_eq!(
            drag.handle(&moved(25, 30)),
            DragOutcome::Moved { x: 25, y: 30 }
        );
        assert_eq!(
            drag.handle(&release(40, 50)),
            DragOutcome::Dropped {
                payload: payload(),
                x: 40,
                y: 50
            }
        );
        assert!(!drag.is_active());
        assert_eq!(drag.handle(&moved(60, 60)), DragOutcome::Ignored);
    }

    #[test]
    fn escape_and_losing_focus_cancel_a_pending_or_running_drag() {
        let escape = Event::KeyPressed {
            key: Key::Escape,
            modifiers: Modifiers::default(),
        };
        let mut drag = DragController::new(4);
        drag.press(0, 0, payload());
        assert_eq!(drag.handle(&escape), DragOutcome::Cancelled);
        assert!(!drag.is_active());

        drag.press(0, 0, payload());
        drag.handle(&moved(9, 0));
        assert_eq!(
            drag.handle(&Event::FocusChanged(false)),
            DragOutcome::Cancelled
        );
        assert_eq!(drag.handle(&release(9, 0)), DragOutcome::Ignored);
        // With nothing in progress, Escape is not ours to take.
        assert_eq!(drag.handle(&escape), DragOutcome::Ignored);
    }

    #[test]
    fn empty_and_oversized_payloads_are_refused() {
        let mut drag = DragController::new(4);
        let empty = DragPayload {
            kind: "files".to_owned(),
            items: Vec::new(),
        };
        assert!(!drag.press(0, 0, empty));
        assert!(!drag.is_active());
        let huge = DragPayload {
            kind: "files".to_owned(),
            items: vec![0; MAX_DRAG_ITEMS + 1],
        };
        assert!(!drag.press(0, 0, huge));
        assert!(!drag.is_active());
    }

    #[test]
    fn other_buttons_and_idle_events_are_ignored() {
        let mut drag = DragController::new(4);
        assert_eq!(drag.handle(&moved(5, 5)), DragOutcome::Ignored);
        assert_eq!(drag.handle(&release(5, 5)), DragOutcome::Ignored);
        drag.press(0, 0, payload());
        let secondary = Event::PointerReleased {
            x: 0,
            y: 0,
            button: PointerButton::Secondary,
        };
        assert_eq!(drag.handle(&secondary), DragOutcome::Ignored);
        assert!(drag.is_active());
    }

    #[test]
    fn extreme_coordinates_do_not_overflow_the_threshold_test() {
        let mut drag = DragController::new(u32::MAX);
        drag.press(i32::MIN, 0, payload());
        assert_eq!(
            drag.handle(&moved(i32::MAX, 0)),
            DragOutcome::Started { x: i32::MAX, y: 0 }
        );
        let mut small = DragController::new(u32::MAX);
        small.press(0, 0, payload());
        assert_eq!(small.handle(&moved(3, 3)), DragOutcome::Pending);
    }
}
