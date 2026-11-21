//! Key presses and pasted text as the bytes a program reads from its terminal.
//!
//! These follow the xterm conventions a shell expects: Enter sends CR,
//! Backspace sends DEL, Ctrl+letter sends the C0 control code, and the
//! cursor keys send `ESC [ A` to `ESC [ D` (normal cursor mode).

use slopos_ui::event::{Key, Modifiers};

/// The most pasted bytes sent at once. Longer text is cut at a character
/// boundary so one paste cannot flood the program.
pub const PASTE_LIMIT: usize = 64 * 1024;

/// The bytes for a key press, or `None` when the key has no terminal meaning
/// (for example a bare modifier or an unsupported function key).
pub fn key_bytes(key: &Key, modifiers: Modifiers) -> Option<Vec<u8>> {
    let bytes: &[u8] = match key {
        Key::Character(ch) => return character_bytes(*ch, modifiers),
        Key::Space => return character_bytes(' ', modifiers),
        Key::Enter => b"\r",
        Key::Escape => b"\x1b",
        Key::Tab if modifiers.shift => b"\x1b[Z",
        Key::Tab => b"\t",
        Key::Backspace => b"\x7f",
        Key::Delete => b"\x1b[3~",
        Key::Left => b"\x1b[D",
        Key::Right => b"\x1b[C",
        Key::Up => b"\x1b[A",
        Key::Down => b"\x1b[B",
        Key::Home => b"\x1b[H",
        Key::End => b"\x1b[F",
        Key::PageUp => b"\x1b[5~",
        Key::PageDown => b"\x1b[6~",
        Key::Function(1) => b"\x1bOP",
        Key::Function(2) => b"\x1bOQ",
        Key::Function(3) => b"\x1bOR",
        Key::Function(4) => b"\x1bOS",
        Key::Function(5) => b"\x1b[15~",
        Key::Function(6) => b"\x1b[17~",
        Key::Function(7) => b"\x1b[18~",
        Key::Function(8) => b"\x1b[19~",
        Key::Function(9) => b"\x1b[20~",
        Key::Function(10) => b"\x1b[21~",
        Key::Function(11) => b"\x1b[23~",
        Key::Function(12) => b"\x1b[24~",
        Key::Function(_) | Key::Other(_) => return None,
    };
    Some(bytes.to_vec())
}

fn character_bytes(ch: char, modifiers: Modifiers) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(5);
    if modifiers.alt {
        // Alt sends ESC before the key, as xterm's metaSendsEscape does.
        out.push(0x1b);
    }
    if modifiers.control {
        out.push(control_code(ch)?);
    } else {
        let mut buffer = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
    }
    Some(out)
}

/// The C0 code Ctrl sends with `ch`: Ctrl+A is 0x01 and so on through Ctrl+Z,
/// with Ctrl+[ as ESC, Ctrl+Space as NUL and Ctrl+? as DEL.
fn control_code(ch: char) -> Option<u8> {
    match ch {
        'a'..='z' | 'A'..='Z' => Some(ch.to_ascii_uppercase() as u8 - b'@'),
        ' ' | '@' => Some(0),
        '['..='_' => Some(ch as u8 & 0x1f),
        '?' => Some(0x7f),
        _ => None,
    }
}

/// The bytes a paste sends. Line breaks become CR, as typing Enter does, and
/// the text is cut to [`PASTE_LIMIT`] bytes at a character boundary.
pub fn paste_bytes(text: &str) -> Vec<u8> {
    let mut end = text.len().min(PASTE_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end]
        .replace("\r\n", "\n")
        .replace('\n', "\r")
        .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Modifiers {
        Modifiers::default()
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            control: true,
            ..Modifiers::default()
        }
    }

    #[test]
    fn enter_backspace_and_tab() {
        assert_eq!(key_bytes(&Key::Enter, plain()), Some(b"\r".to_vec()));
        assert_eq!(key_bytes(&Key::Backspace, plain()), Some(vec![0x7f]));
        assert_eq!(key_bytes(&Key::Tab, plain()), Some(b"\t".to_vec()));
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(key_bytes(&Key::Tab, shift), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn ctrl_letters_send_control_codes() {
        assert_eq!(key_bytes(&Key::Character('c'), ctrl()), Some(vec![0x03]));
        assert_eq!(key_bytes(&Key::Character('D'), ctrl()), Some(vec![0x04]));
        assert_eq!(key_bytes(&Key::Character('['), ctrl()), Some(vec![0x1b]));
        assert_eq!(key_bytes(&Key::Space, ctrl()), Some(vec![0]));
        assert_eq!(key_bytes(&Key::Character('1'), ctrl()), None);
    }

    #[test]
    fn cursor_and_editing_keys_use_xterm_sequences() {
        assert_eq!(key_bytes(&Key::Up, plain()), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_bytes(&Key::Left, plain()), Some(b"\x1b[D".to_vec()));
        assert_eq!(key_bytes(&Key::Delete, plain()), Some(b"\x1b[3~".to_vec()));
        assert_eq!(key_bytes(&Key::PageUp, plain()), Some(b"\x1b[5~".to_vec()));
        assert_eq!(
            key_bytes(&Key::Function(1), plain()),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(key_bytes(&Key::Function(20), plain()), None);
        assert_eq!(key_bytes(&Key::Other(0xffe1), plain()), None);
    }

    #[test]
    fn text_is_sent_as_utf8_and_alt_prefixes_escape() {
        assert_eq!(
            key_bytes(&Key::Character('é'), plain()),
            Some("é".as_bytes().to_vec())
        );
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        assert_eq!(
            key_bytes(&Key::Character('x'), alt),
            Some(b"\x1bx".to_vec())
        );
    }

    #[test]
    fn paste_turns_line_breaks_into_carriage_returns_and_is_capped() {
        assert_eq!(
            paste_bytes("one\ntwo\r\nthree"),
            b"one\rtwo\rthree".to_vec()
        );
        let long = "é".repeat(PASTE_LIMIT);
        let pasted = paste_bytes(&long);
        assert!(pasted.len() <= PASTE_LIMIT);
        assert!(std::str::from_utf8(&pasted).is_ok());
    }
}
