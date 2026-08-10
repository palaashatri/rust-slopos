use crate::Point;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum Event {
    MouseDown {
        button: MouseButton,
        point: Point,
        modifiers: Modifiers,
    },
    MouseUp {
        button: MouseButton,
        point: Point,
        modifiers: Modifiers,
    },
    MouseMove {
        point: Point,
        modifiers: Modifiers,
    },
    MouseEnter,
    MouseLeave,
    Click {
        button: MouseButton,
        point: Point,
        modifiers: Modifiers,
    },
    DoubleClick {
        button: MouseButton,
        point: Point,
        modifiers: Modifiers,
    },
    KeyDown {
        key: KeyCode,
        modifiers: Modifiers,
    },
    KeyUp {
        key: KeyCode,
        modifiers: Modifiers,
    },
    Char {
        character: char,
    },
    FocusIn,
    FocusOut,
    Scroll {
        delta: Point,
        modifiers: Modifiers,
    },
    DragStart {
        point: Point,
    },
    Drag {
        point: Point,
    },
    DragEnd {
        point: Point,
    },
    Drop {
        point: Point,
    },
    LayoutChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Escape,
    Tab,
    CapsLock,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    Space,
    Enter,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    MetaLeft,
    MetaRight,
    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        control: false,
        alt: false,
        meta: false,
    };
}

#[derive(Debug, Clone)]
pub enum EventResult {
    Handled,
    Ignored,
    StopPropagation,
    RequestRedraw,
}

pub trait EventHandler {
    fn handle_event(&mut self, event: &Event) -> EventResult;
}
