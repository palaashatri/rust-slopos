//! The IBus wire format: service names, capability and state bits, the
//! `IBusText` value that carries text, and the mapping from the input
//! context's signals to [`InputMethodEvent`]s. No sockets here, so every
//! function is testable without a daemon.
//!
//! IBus serialises its objects as GVariant structures whose first field is
//! the class name (`"IBusText"`), then an `a{sv}` of attachments, then the
//! fields. Layouts used here, following the IBus sources:
//!
//! - `IBusText`: `(s a{sv} s v)`: text, then an `IBusAttrList` variant.
//! - `IBusAttrList`: `(s a{sv} av)`: the attribute variants.
//! - `IBusAttribute`: `(s a{sv} u u u u)`: type, value, start and end.

use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use crate::input_method::{byte_offset_of_char, InputMethodEvent};

/// Well-known name of the IBus daemon's bus service.
pub const SERVICE: &str = "org.freedesktop.IBus";
/// Object path of the bus service, where input contexts are created.
pub const BUS_PATH: &str = "/org/freedesktop/IBus";
/// Interface of the bus service.
pub const BUS_INTERFACE: &str = "org.freedesktop.IBus";
/// Interface of one input context (a text field's conversation with IBus).
pub const CONTEXT_INTERFACE: &str = "org.freedesktop.IBus.InputContext";

/// `IBUS_CAP_PREEDIT_TEXT`: the client draws the composition itself.
pub const CAP_PREEDIT_TEXT: u32 = 1 << 0;
/// `IBUS_CAP_FOCUS`: the client reports focus changes.
pub const CAP_FOCUS: u32 = 1 << 3;

/// `IBUS_ATTR_TYPE_UNDERLINE`.
pub const ATTR_TYPE_UNDERLINE: u32 = 1;
/// `IBUS_ATTR_UNDERLINE_SINGLE`.
pub const ATTR_UNDERLINE_SINGLE: u32 = 1;

/// `IBUS_RELEASE_MASK`: set in the state of key releases. SLOPOS forwards
/// key presses only.
pub const STATE_RELEASE_MASK: u32 = 1 << 30;

/// The capabilities SLOPOS advertises for its input contexts.
pub const fn capabilities() -> u32 {
    CAP_PREEDIT_TEXT | CAP_FOCUS
}

/// The `v` argument of `CommitText`, `UpdatePreeditText` and friends:
/// `text` as an `IBusText`, underlined over its whole length when
/// `underline` is set (how a composition is shown).
pub fn encode_text(text: &str, underline: bool) -> Value<'static> {
    let chars = text.chars().count() as u32;
    let mut attributes: Vec<Value<'static>> = Vec::new();
    if underline && chars > 0 {
        let attribute = (
            "IBusAttribute",
            HashMap::<String, OwnedValue>::new(),
            ATTR_TYPE_UNDERLINE,
            ATTR_UNDERLINE_SINGLE,
            0u32,
            chars,
        );
        attributes.push(Value::new(attribute));
    }
    let attr_list = (
        "IBusAttrList",
        HashMap::<String, OwnedValue>::new(),
        attributes,
    );
    let ibus_text = (
        "IBusText",
        HashMap::<String, OwnedValue>::new(),
        text.to_owned(),
        Value::new(attr_list),
    );
    Value::Value(Box::new(Value::new(ibus_text)))
}

/// The text inside an `IBusText` value, or `None` when `value` is not one.
pub fn decode_text(value: &Value<'_>) -> Option<String> {
    match value {
        Value::Value(inner) => decode_text(inner),
        Value::Structure(structure) => {
            let fields = structure.fields();
            match (fields.first(), fields.get(2)) {
                (Some(Value::Str(class)), Some(Value::Str(text)))
                    if class.as_str() == "IBusText" =>
                {
                    Some(text.as_str().to_owned())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// `CommitText(v text)`: the committed text, or `None` for a malformed value.
pub fn commit_event(text: &Value<'_>) -> Option<InputMethodEvent> {
    decode_text(text).map(InputMethodEvent::Commit)
}

/// `UpdatePreeditText(v text, u cursor_pos, b visible)`. `cursor_pos` counts
/// characters; the event carries a byte offset. Hidden or empty compositions
/// become [`InputMethodEvent::PreeditHidden`]. `None` for a malformed value.
pub fn preedit_event(
    text: &Value<'_>,
    cursor_chars: u32,
    visible: bool,
) -> Option<InputMethodEvent> {
    let text = decode_text(text)?;
    if !visible || text.is_empty() {
        return Some(InputMethodEvent::PreeditHidden);
    }
    let cursor = byte_offset_of_char(&text, cursor_chars);
    Some(InputMethodEvent::Preedit { text, cursor })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_bits_match_ibus_constants() {
        assert_eq!(capabilities(), 0b1001);
    }

    #[test]
    fn encoded_text_round_trips_with_its_underline() {
        let value = encode_text("你好a", true);
        assert_eq!(decode_text(&value).as_deref(), Some("你好a"));
        assert_eq!(decode_text(&encode_text("", false)).as_deref(), Some(""));
    }

    #[test]
    fn encoded_text_has_the_ibus_layout() {
        let Value::Value(inner) = encode_text("ab", true) else {
            panic!("IBusText is sent as a variant");
        };
        assert_eq!(inner.value_signature(), "(sa{sv}sv)");
        let Value::Structure(text) = *inner else {
            panic!("IBusText is a structure");
        };
        let Value::Value(attr_list) = &text.fields()[3] else {
            panic!("the attribute list is a variant");
        };
        assert_eq!(attr_list.value_signature(), "(sa{sv}av)");
        let Value::Structure(list) = &**attr_list else {
            panic!("IBusAttrList is a structure");
        };
        let Value::Array(attributes) = &list.fields()[2] else {
            panic!("the attributes are an array");
        };
        assert_eq!(attributes.len(), 1);
    }

    #[test]
    fn decode_rejects_other_values() {
        assert_eq!(decode_text(&Value::from(7u32)), None);
        let other = Value::new(("IBusAttribute", HashMap::<String, OwnedValue>::new(), "x"));
        assert_eq!(decode_text(&other), None);
    }

    #[test]
    fn commit_signal_maps_to_a_commit() {
        assert_eq!(
            commit_event(&encode_text("阿", false)),
            Some(InputMethodEvent::Commit("阿".into()))
        );
        assert_eq!(commit_event(&Value::from(1u8)), None);
    }

    #[test]
    fn preedit_signal_maps_cursor_chars_to_bytes() {
        assert_eq!(
            preedit_event(&encode_text("你好", true), 1, true),
            Some(InputMethodEvent::Preedit {
                text: "你好".into(),
                cursor: "你".len(),
            })
        );
        // The cursor past the end lands at the end.
        assert_eq!(
            preedit_event(&encode_text("ab", true), 9, true),
            Some(InputMethodEvent::Preedit {
                text: "ab".into(),
                cursor: 2,
            })
        );
    }

    #[test]
    fn hidden_or_empty_preedit_maps_to_hidden() {
        assert_eq!(
            preedit_event(&encode_text("ab", true), 0, false),
            Some(InputMethodEvent::PreeditHidden)
        );
        assert_eq!(
            preedit_event(&encode_text("", true), 0, true),
            Some(InputMethodEvent::PreeditHidden)
        );
        assert_eq!(preedit_event(&Value::from(true), 0, true), None);
    }

    #[test]
    fn names_are_the_ibus_interfaces() {
        assert_eq!(SERVICE, "org.freedesktop.IBus");
        assert_eq!(BUS_PATH, "/org/freedesktop/IBus");
        assert_eq!(CONTEXT_INTERFACE, "org.freedesktop.IBus.InputContext");
    }
}
