//! Input-method (IME) types shared by the toolkit and its hosts.
//!
//! Nothing here depends on X11 or D-Bus. The IBus client that produces these
//! events lives in [`crate::platform::ibus`]; text fields consume them through
//! [`crate::controls::text_field::TextField::handle`].

use std::fmt;

/// Text an input method sends to the focused text field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMethodEvent {
    /// The input method committed `text`: insert it at the caret. The
    /// composition, if any, ends.
    Commit(String),
    /// The composition (preedit) text is now `text`, shown underlined at the
    /// caret and not yet part of the field's value. `cursor` is the byte
    /// offset of the insertion point inside `text`; the field clamps it to a
    /// char boundary.
    Preedit { text: String, cursor: usize },
    /// The composition ended without a commit (cancelled, or hidden by the
    /// input method).
    PreeditHidden,
}

/// Whether an input method is driving text entry for this window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMethodStatus {
    /// The input method daemon accepted the input context.
    Active,
    /// No input method is used; keys go straight to the application. The
    /// reason says why, for diagnostics and the status line.
    Unavailable(String),
}

impl fmt::Display for InputMethodStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Unavailable(reason) => write!(f, "unavailable: {reason}"),
        }
    }
}

/// What a host hands the application from its input method: text, or the
/// status of the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Input(InputMethodEvent),
    Status(InputMethodStatus),
}

/// Convert an IME cursor position counted in characters (IBus counts chars,
/// not bytes) into a byte offset inside `text`. Positions past the end clamp
/// to `text.len()`.
pub fn byte_offset_of_char(text: &str, chars: u32) -> usize {
    text.char_indices()
        .nth(chars as usize)
        .map_or(text.len(), |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reads_as_active_or_unavailable_with_reason() {
        assert_eq!(InputMethodStatus::Active.to_string(), "active");
        assert_eq!(
            InputMethodStatus::Unavailable("no IBus bus address".into()).to_string(),
            "unavailable: no IBus bus address"
        );
    }

    #[test]
    fn char_positions_become_byte_offsets() {
        let text = "a你b";
        assert_eq!(byte_offset_of_char(text, 0), 0);
        assert_eq!(byte_offset_of_char(text, 1), 1);
        assert_eq!(byte_offset_of_char(text, 2), 4);
        assert_eq!(byte_offset_of_char(text, 3), text.len());
        assert_eq!(byte_offset_of_char(text, 99), text.len());
        assert_eq!(byte_offset_of_char("", 0), 0);
    }
}
