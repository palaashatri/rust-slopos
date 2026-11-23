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
pub fn key_bytes(key: &Key, modifiers: Modifiers, application_cursor: bool) -> Option<Vec<u8>> {
    // With application cursor keys (`CSI ?1 h`), an unmodified arrow, Home or
    // End is SS3 instead of CSI. Modified ones keep the CSI form.
    let unmodified = !modifiers.shift && !modifiers.control && !modifiers.alt;
    if application_cursor && unmodified {
        let ss3: Option<&[u8]> = match key {
            Key::Up => Some(b"\x1bOA"),
            Key::Down => Some(b"\x1bOB"),
            Key::Right => Some(b"\x1bOC"),
            Key::Left => Some(b"\x1bOD"),
            Key::Home => Some(b"\x1bOH"),
            Key::End => Some(b"\x1bOF"),
            _ => None,
        };
        if let Some(bytes) = ss3 {
            return Some(bytes.to_vec());
        }
    }
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
///
/// With bracketed paste on (`CSI ?2004 h`) the text is wrapped in the paste
/// markers, so the shell does not run its lines as they arrive, and any escape
/// characters in it are removed so it cannot end the paste early. Without it,
/// a multi-line paste is refused: each line break would run a command.
pub fn paste_bytes(text: &str, bracketed: bool) -> Option<Vec<u8>> {
    let mut end = text.len().min(PASTE_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let text = &text[..end];
    if bracketed {
        let body: String = text
            .replace("\r\n", "\n")
            .replace('\n', "\r")
            .chars()
            .filter(|ch| *ch != '\x1b')
            .collect();
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(body.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        return Some(out);
    }
    if text.contains('\n') || text.contains('\r') {
        return None;
    }
    Some(text.as_bytes().to_vec())
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
        assert_eq!(key_bytes(&Key::Enter, plain(), false), Some(b"\r".to_vec()));
        assert_eq!(key_bytes(&Key::Backspace, plain(), false), Some(vec![0x7f]));
        assert_eq!(key_bytes(&Key::Tab, plain(), false), Some(b"\t".to_vec()));
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(key_bytes(&Key::Tab, shift, false), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn ctrl_letters_send_control_codes() {
        assert_eq!(
            key_bytes(&Key::Character('c'), ctrl(), false),
            Some(vec![0x03])
        );
        assert_eq!(
            key_bytes(&Key::Character('D'), ctrl(), false),
            Some(vec![0x04])
        );
        assert_eq!(
            key_bytes(&Key::Character('['), ctrl(), false),
            Some(vec![0x1b])
        );
        assert_eq!(key_bytes(&Key::Space, ctrl(), false), Some(vec![0]));
        assert_eq!(key_bytes(&Key::Character('1'), ctrl(), false), None);
    }

    #[test]
    fn cursor_and_editing_keys_use_xterm_sequences() {
        assert_eq!(
            key_bytes(&Key::Up, plain(), false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            key_bytes(&Key::Left, plain(), false),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            key_bytes(&Key::Delete, plain(), false),
            Some(b"\x1b[3~".to_vec())
        );
        assert_eq!(
            key_bytes(&Key::PageUp, plain(), false),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            key_bytes(&Key::Function(1), plain(), false),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(key_bytes(&Key::Function(20), plain(), false), None);
        assert_eq!(key_bytes(&Key::Other(0xffe1), plain(), false), None);
    }

    #[test]
    fn text_is_sent_as_utf8_and_alt_prefixes_escape() {
        assert_eq!(
            key_bytes(&Key::Character('é'), plain(), false),
            Some("é".as_bytes().to_vec())
        );
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        assert_eq!(
            key_bytes(&Key::Character('x'), alt, false),
            Some(b"\x1bx".to_vec())
        );
    }

    #[test]
    fn bracketed_paste_wraps_the_text_and_turns_line_breaks_into_carriage_returns() {
        assert_eq!(
            paste_bytes("one\ntwo\r\nthree", true),
            Some(b"\x1b[200~one\rtwo\rthree\x1b[201~".to_vec())
        );
        let long = "é".repeat(PASTE_LIMIT);
        let pasted = paste_bytes(&long, true).unwrap();
        assert!(pasted.len() <= PASTE_LIMIT + 12);
        assert!(std::str::from_utf8(&pasted[6..pasted.len() - 6]).is_ok());
    }

    #[test]
    fn escape_characters_cannot_end_a_bracketed_paste_early() {
        assert_eq!(
            paste_bytes("a\x1b[201~b", true),
            Some(b"\x1b[200~a[201~b\x1b[201~".to_vec())
        );
    }

    #[test]
    fn a_multi_line_paste_is_refused_without_bracketed_paste() {
        assert_eq!(paste_bytes("one\ntwo", false), None);
        assert_eq!(paste_bytes("one", false), Some(b"one".to_vec()));
    }

    #[test]
    fn application_cursor_keys_use_ss3_only_when_unmodified() {
        assert_eq!(key_bytes(&Key::Up, plain(), true), Some(b"\x1bOA".to_vec()));
        assert_eq!(
            key_bytes(&Key::Up, plain(), false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            key_bytes(&Key::Up, ctrl(), true),
            key_bytes(&Key::Up, ctrl(), false)
        );
    }
}
