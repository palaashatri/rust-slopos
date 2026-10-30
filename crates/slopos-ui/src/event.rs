//! Input and window events delivered to applications, in logical pixels.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Middle,
    Secondary,
    Other(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub logo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Character(char),
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
    /// A key with no SLOPOS meaning; carries the X keysym.
    Other(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    PointerMoved {
        x: i32,
        y: i32,
    },
    PointerPressed {
        x: i32,
        y: i32,
        button: PointerButton,
    },
    PointerReleased {
        x: i32,
        y: i32,
        button: PointerButton,
    },
    PointerLeft,
    /// Wheel steps: positive `dy` scrolls down.
    Scroll {
        dx: i32,
        dy: i32,
    },
    KeyPressed {
        key: Key,
        modifiers: Modifiers,
    },
    FocusChanged(bool),
    /// New logical size of the window content.
    Resized {
        width: u32,
        height: u32,
    },
    CloseRequested,
    /// A timer started with [`crate::platform::Control::start_timer`] fired.
    Timer(u64),
    /// Another thread called [`crate::platform::Proxy::wake`].
    Wake,
    /// An assistive technology asked for an action on a node.
    Accessibility(AccessibilityRequest),
    /// Clipboard text arrived after [`crate::platform::Control::request_paste`].
    Paste(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessibilityRequest {
    /// Activate the node (press a button).
    Click(u64),
    /// Move keyboard focus to the node.
    Focus(u64),
    /// Replace the value of an editable node (text fields).
    SetValue(u64, String),
    /// Step a range control (sliders) up or down.
    Increment(u64),
    Decrement(u64),
}
