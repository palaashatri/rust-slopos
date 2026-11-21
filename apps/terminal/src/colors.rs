//! Terminal colours in pixels: the xterm 256-colour table, and the theme's
//! ink and background for the default colours.

use slopos_ui::foundation::color::Color as Rgb;

use crate::grid::Color;

/// The first 16 entries (the basic and bright colours). These are the usual
/// xterm values; the theme does not restyle them yet.
const BASIC: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00),
    (0xcd, 0x00, 0x00),
    (0x00, 0xcd, 0x00),
    (0xcd, 0xcd, 0x00),
    (0x00, 0x00, 0xee),
    (0xcd, 0x00, 0xcd),
    (0x00, 0xcd, 0xcd),
    (0xe5, 0xe5, 0xe5),
    (0x7f, 0x7f, 0x7f),
    (0xff, 0x00, 0x00),
    (0x00, 0xff, 0x00),
    (0xff, 0xff, 0x00),
    (0x5c, 0x5c, 0xff),
    (0xff, 0x00, 0xff),
    (0x00, 0xff, 0xff),
    (0xff, 0xff, 0xff),
];

/// The RGB value of xterm 256-colour entry `index`.
pub fn indexed_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => BASIC[usize::from(index)],
        16..=231 => {
            // A 6x6x6 colour cube; each axis steps 0, 95, 135, 175, 215, 255.
            let cube = index - 16;
            let level = |step: u8| if step == 0 { 0 } else { 55 + 40 * step };
            (level(cube / 36), level((cube / 6) % 6), level(cube % 6))
        }
        232..=255 => {
            let value = 8 + 10 * (index - 232);
            (value, value, value)
        }
    }
}

/// A terminal colour as pixels; the default colour is `default`.
pub fn resolve(color: Color, default: Rgb) -> Rgb {
    match color {
        Color::Default => default,
        Color::Indexed(index) => {
            let (red, green, blue) = indexed_rgb(index);
            Rgb {
                red,
                green,
                blue,
                alpha: 255,
            }
        }
        Color::Rgb(red, green, blue) => Rgb {
            red,
            green,
            blue,
            alpha: 255,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cube_and_greys_follow_the_xterm_table() {
        assert_eq!(indexed_rgb(16), (0, 0, 0));
        assert_eq!(indexed_rgb(196), (255, 0, 0));
        assert_eq!(indexed_rgb(231), (255, 255, 255));
        assert_eq!(indexed_rgb(232), (8, 8, 8));
        assert_eq!(indexed_rgb(255), (238, 238, 238));
        assert_eq!(indexed_rgb(9), (255, 0, 0));
    }

    #[test]
    fn true_colours_are_used_as_given() {
        let ink = Rgb {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        assert_eq!(
            resolve(Color::Rgb(18, 52, 86), ink),
            Rgb {
                red: 18,
                green: 52,
                blue: 86,
                alpha: 255
            }
        );
    }

    #[test]
    fn the_default_colour_comes_from_the_theme() {
        let ink = Rgb {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        };
        assert_eq!(resolve(Color::Default, ink), ink);
        assert_eq!(
            resolve(Color::Indexed(2), ink),
            Rgb {
                red: 0,
                green: 0xcd,
                blue: 0,
                alpha: 255
            }
        );
    }
}
