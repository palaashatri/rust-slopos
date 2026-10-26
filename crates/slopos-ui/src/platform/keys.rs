//! X11 keysym → [`Key`] translation.

use crate::event::{Key, Modifiers};

/// Modifier bits of an X11 key or button event state.
pub fn modifiers(state: u16) -> Modifiers {
    Modifiers {
        shift: state & 0x0001 != 0,
        control: state & 0x0004 != 0,
        alt: state & 0x0008 != 0,
        logo: state & 0x0040 != 0,
    }
}

/// Choose the keysym for a key event from its keyboard-mapping row.
///
/// `num_lock` is the modifier bit the server assigned to Num Lock (usually
/// Mod2). With Num Lock on, a keypad key's shifted (numeric) keysym wins;
/// Shift reverses that, so the navigation keys stay reachable.
pub fn keysym_for(row: &[u32], state: u16, num_lock: u16) -> u32 {
    let shift = state & 0x0001 != 0;
    let caps_lock = state & 0x0002 != 0;
    let num_lock = num_lock != 0 && state & num_lock != 0;
    let lower = row.first().copied().unwrap_or(0);
    let upper = row
        .get(1)
        .copied()
        .filter(|keysym| *keysym != 0)
        .unwrap_or(lower);
    let is_keypad = |keysym: u32| (0xff80..=0xffbd).contains(&keysym);
    if num_lock && is_keypad(upper) {
        return if shift { lower } else { upper };
    }
    let is_letter = char::from_u32(lower).is_some_and(|c| c.is_ascii_lowercase());
    if shift ^ (caps_lock && is_letter) {
        upper
    } else {
        lower
    }
}

pub fn key_for(keysym: u32) -> Key {
    match keysym {
        0xff0d | 0xff8d => Key::Enter,
        0xff1b => Key::Escape,
        0xff09 | 0xfe20 => Key::Tab,
        0xff08 => Key::Backspace,
        0xffff | 0xff9f => Key::Delete,
        0x0020 => Key::Space,
        0xff51 | 0xff96 => Key::Left,
        0xff53 | 0xff98 => Key::Right,
        0xff52 | 0xff97 => Key::Up,
        0xff54 | 0xff99 => Key::Down,
        0xff50 | 0xff95 => Key::Home,
        0xff57 | 0xff9c => Key::End,
        0xff55 | 0xff9a => Key::PageUp,
        0xff56 | 0xff9b => Key::PageDown,
        0xffbe..=0xffd5 => Key::Function((keysym - 0xffbe + 1) as u8),
        // Keypad digits and operators.
        0xffb0..=0xffb9 => Key::Character(char::from(b'0' + (keysym - 0xffb0) as u8)),
        0xffaa => Key::Character('*'),
        0xffab => Key::Character('+'),
        0xffad => Key::Character('-'),
        0xffae => Key::Character('.'),
        0xffaf => Key::Character('/'),
        // Latin-1 keysyms equal their code points.
        0x0021..=0x007e | 0x00a0..=0x00ff => char::from_u32(keysym)
            .map(Key::Character)
            .unwrap_or(Key::Other(keysym)),
        // Unicode keysyms.
        0x0100_0000..=0x0110_ffff => char::from_u32(keysym - 0x0100_0000)
            .map(Key::Character)
            .unwrap_or(Key::Other(keysym)),
        other => Key::Other(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_and_caps_lock_pick_the_right_level() {
        let row = [u32::from(b'a'), u32::from(b'A')];
        assert_eq!(keysym_for(&row, 0, 0), u32::from(b'a'));
        assert_eq!(keysym_for(&row, 0x0001, 0), u32::from(b'A'));
        assert_eq!(keysym_for(&row, 0x0002, 0), u32::from(b'A'));
        assert_eq!(keysym_for(&row, 0x0003, 0), u32::from(b'a'));
        let digit = [u32::from(b'1'), u32::from(b'!')];
        assert_eq!(keysym_for(&digit, 0x0002, 0), u32::from(b'1'));
    }

    #[test]
    fn num_lock_selects_the_keypad_level() {
        // A standard keypad row is [navigation, digit].
        let keypad = [0xff95, 0xffb7]; // KP_Home, KP_7
        assert_eq!(keysym_for(&keypad, 0, 0x0010), 0xff95);
        assert_eq!(keysym_for(&keypad, 0x0010, 0x0010), 0xffb7);
        assert_eq!(keysym_for(&keypad, 0x0011, 0x0010), 0xff95);
        assert_eq!(keysym_for(&keypad, 0x0001, 0x0010), 0xffb7);
        // Num Lock never changes the level of ordinary keys.
        let letter = [u32::from(b'a'), u32::from(b'A')];
        assert_eq!(keysym_for(&letter, 0x0010, 0x0010), u32::from(b'a'));
        assert_eq!(keysym_for(&letter, 0x0002, 0x0010), u32::from(b'a'));
    }

    #[test]
    fn keysyms_map_to_keys() {
        assert_eq!(key_for(0xff0d), Key::Enter);
        assert_eq!(key_for(0xff8d), Key::Enter);
        assert_eq!(key_for(u32::from(b'7')), Key::Character('7'));
        assert_eq!(key_for(0xffb7), Key::Character('7'));
        assert_eq!(key_for(0xffab), Key::Character('+'));
        assert_eq!(key_for(0x0100_20ac), Key::Character('€'));
        assert_eq!(key_for(0xffbe), Key::Function(1));
        assert_eq!(key_for(0xffe1), Key::Other(0xffe1));
    }

    #[test]
    fn modifier_bits_are_decoded() {
        let mods = modifiers(0x0001 | 0x0004 | 0x0040);
        assert!(mods.shift && mods.control && mods.logo && !mods.alt);
    }
}
