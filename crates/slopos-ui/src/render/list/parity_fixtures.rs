//! Draw lists shared by the CPU parity tests and the GPU parity suite.
//!
//! The first group is the phase-A suite from the parent module's tests, built
//! again here with the same geometry. The last group is the phase-B addition:
//! images at non-integer scales, which take the bicubic path and are where
//! the GPU executor is expected to differ from the reference.

use super::{DrawCommand, DrawList};
use crate::foundation::{color::Color, geometry::Rect};
use crate::image::Image;
use crate::text::Font;

/// One fixed draw list with the logical surface and scale it is rendered at.
pub(crate) struct Scenario {
    pub name: &'static str,
    pub list: DrawList,
    pub width: u32,
    pub height: u32,
    pub factor: u32,
}

const BLACK: Color = Color::BLACK;
const HALF_RED: Color = Color {
    red: 255,
    green: 0,
    blue: 0,
    alpha: 128,
};

fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn list_of(commands: &[DrawCommand]) -> DrawList {
    let mut list = DrawList::new(64);
    for command in commands {
        list.push(command.clone()).unwrap();
    }
    list
}

fn scenario(name: &'static str, list: DrawList, width: u32, height: u32, factor: u32) -> Scenario {
    Scenario {
        name,
        list,
        width,
        height,
        factor,
    }
}

/// Opaque 2x2 checkerboard: black, white, red, blue.
fn checker() -> Image {
    Image::from_rgba(
        2,
        2,
        vec![
            0, 0, 0, 255, // black
            255, 255, 255, 255, // white
            255, 0, 0, 255, // red
            0, 0, 255, 255, // blue
        ],
    )
    .unwrap()
}

/// Translucent 3x2 image, so the bicubic path sees partial alpha too.
fn gradient() -> Image {
    Image::from_rgba(
        3,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, //
            0, 0, 0, 255, 128, 128, 128, 200, 255, 255, 255, 255,
        ],
    )
    .unwrap()
}

/// Every phase-A scenario, then the phase-B additions.
pub(crate) fn scenarios() -> Vec<Scenario> {
    let mut all = phase_a();
    all.extend(phase_b());
    all
}

fn phase_a() -> Vec<Scenario> {
    let font = Font::system(12.0);
    let mut out = Vec::new();

    for factor in [1u32, 2] {
        let name = if factor == 1 { "fill_1x" } else { "fill_2x" };
        out.push(scenario(
            name,
            list_of(&[DrawCommand::FillRect {
                rect: rect(1, 1, 4, 3),
                color: BLACK,
            }]),
            8,
            6,
            factor,
        ));
    }

    out.push(scenario(
        "rounded_1x",
        list_of(&[DrawCommand::RoundedRect {
            rect: rect(0, 0, 8, 6),
            radius: 2,
            color: BLACK,
        }]),
        8,
        6,
        1,
    ));
    out.push(scenario(
        "rounded_2x",
        list_of(&[DrawCommand::RoundedRect {
            rect: rect(0, 0, 8, 6),
            radius: 2,
            color: BLACK,
        }]),
        16,
        12,
        2,
    ));
    out.push(scenario(
        "rounded_golden_1x",
        list_of(&[DrawCommand::RoundedRect {
            rect: rect(1, 1, 6, 4),
            radius: 2,
            color: BLACK,
        }]),
        8,
        6,
        1,
    ));
    out.push(scenario(
        "rounded_golden_2x",
        list_of(&[DrawCommand::RoundedRect {
            rect: rect(1, 1, 6, 4),
            radius: 2,
            color: BLACK,
        }]),
        16,
        12,
        2,
    ));
    out.push(scenario(
        "rounded_radius_clamped",
        list_of(&[DrawCommand::RoundedRect {
            rect: rect(0, 0, 8, 6),
            radius: 100,
            color: HALF_RED,
        }]),
        8,
        6,
        1,
    ));

    out.push(scenario(
        "stroke_1x",
        list_of(&[DrawCommand::StrokeRect {
            rect: rect(0, 0, 5, 4),
            color: BLACK,
        }]),
        8,
        6,
        1,
    ));
    out.push(scenario(
        "stroke_2x",
        list_of(&[DrawCommand::StrokeRect {
            rect: rect(0, 0, 5, 4),
            color: BLACK,
        }]),
        16,
        12,
        2,
    ));
    out.push(scenario(
        "stroke_thin_edges",
        list_of(&[
            DrawCommand::StrokeRect {
                rect: rect(0, 0, 1, 3),
                color: BLACK,
            },
            DrawCommand::StrokeRect {
                rect: rect(3, 0, 3, 1),
                color: BLACK,
            },
        ]),
        8,
        4,
        1,
    ));
    out.push(scenario(
        "stroke_translucent",
        list_of(&[DrawCommand::StrokeRect {
            rect: rect(0, 0, 5, 4),
            color: HALF_RED,
        }]),
        5,
        4,
        1,
    ));
    out.push(scenario(
        "translucent_fill",
        list_of(&[DrawCommand::FillRect {
            rect: rect(0, 0, 2, 1),
            color: HALF_RED,
        }]),
        2,
        1,
        1,
    ));
    out.push(scenario(
        "translucent_rounded",
        list_of(&[DrawCommand::RoundedRect {
            rect: rect(0, 0, 8, 6),
            radius: 2,
            color: HALF_RED,
        }]),
        8,
        6,
        1,
    ));

    for factor in [1u32, 2] {
        let (name, width, height) = if factor == 1 {
            ("text_1x", 48, 24)
        } else {
            ("text_2x", 48, 24)
        };
        out.push(scenario(
            name,
            list_of(&[DrawCommand::TextRun {
                x: 1,
                y: 1,
                text: "SLOPOS".to_string(),
                font: font.clone(),
                color: BLACK,
            }]),
            width,
            height,
            factor,
        ));
    }

    for factor in [1u32, 2] {
        let (name, width, height) = if factor == 1 {
            ("image_1x", 6, 5)
        } else {
            ("image_2x", 6, 5)
        };
        let mut list = DrawList::new(8);
        let handle = list.add_image(checker()).unwrap();
        list.push(DrawCommand::ImageBlit {
            image: handle,
            rect: rect(1, 1, 2, 2),
            opacity: 255,
        })
        .unwrap();
        out.push(scenario(name, list, width, height, factor));
    }

    let mut opacity = DrawList::new(8);
    let red = opacity
        .add_image(Image::from_rgba(1, 1, vec![255, 0, 0, 255]).unwrap())
        .unwrap();
    opacity
        .push(DrawCommand::ImageBlit {
            image: red,
            rect: rect(0, 0, 1, 1),
            opacity: 128,
        })
        .unwrap();
    out.push(scenario("image_opacity_half", opacity, 2, 1, 1));

    out.push(scenario("clip_limits_all_kinds", clip_all_kinds(), 8, 8, 1));
    out.push(scenario(
        "clip_limits_all_kinds_2x",
        clip_all_kinds(),
        8,
        8,
        2,
    ));
    out.push(scenario(
        "nested_clips",
        list_of(&[
            DrawCommand::PushClip {
                rect: rect(2, 2, 6, 6),
            },
            DrawCommand::PushClip {
                rect: rect(4, 0, 20, 20),
            },
            DrawCommand::FillRect {
                rect: rect(0, 0, 12, 10),
                color: BLACK,
            },
            DrawCommand::PopClip,
            DrawCommand::PopClip,
        ]),
        12,
        10,
        1,
    ));
    for factor in [1u32, 2] {
        out.push(scenario(
            if factor == 1 {
                "clamp_at_edges_1x"
            } else {
                "clamp_at_edges_2x"
            },
            list_of(&[
                DrawCommand::FillRect {
                    rect: rect(-2, -2, 4, 4),
                    color: BLACK,
                },
                DrawCommand::PushClip {
                    rect: rect(-100, -100, 1000, 1000),
                },
                DrawCommand::FillRect {
                    rect: rect(0, 0, 4, 4),
                    color: BLACK,
                },
                DrawCommand::PopClip,
            ]),
            4,
            4,
            factor,
        ));
    }
    out.push(scenario(
        "clip_golden_1x",
        list_of(&[
            DrawCommand::PushClip {
                rect: rect(2, 2, 3, 3),
            },
            DrawCommand::FillRect {
                rect: rect(0, 0, 8, 6),
                color: BLACK,
            },
            DrawCommand::PopClip,
        ]),
        8,
        6,
        1,
    ));
    out.push(scenario(
        "clip_golden_2x",
        list_of(&[
            DrawCommand::PushClip {
                rect: rect(2, 2, 3, 3),
            },
            DrawCommand::FillRect {
                rect: rect(0, 0, 8, 6),
                color: BLACK,
            },
            DrawCommand::PopClip,
        ]),
        16,
        12,
        2,
    ));

    out.push(scenario("hidpi_mixed_1x", hidpi_mixed(), 10, 8, 1));
    out.push(scenario("hidpi_mixed_2x", hidpi_mixed(), 10, 8, 2));
    out
}

fn clip_all_kinds() -> DrawList {
    let font = Font::system(10.0);
    let mut list = DrawList::new(32);
    let handle = list.add_image(checker()).unwrap();
    for command in [
        DrawCommand::PushClip {
            rect: rect(0, 0, 4, 4),
        },
        DrawCommand::RoundedRect {
            rect: rect(-2, -2, 12, 12),
            radius: 3,
            color: BLACK,
        },
        DrawCommand::StrokeRect {
            rect: rect(0, 0, 8, 8),
            color: BLACK,
        },
        DrawCommand::ImageBlit {
            image: handle,
            rect: rect(3, 3, 4, 4),
            opacity: 255,
        },
        DrawCommand::TextRun {
            x: 1,
            y: 1,
            text: "SLOPOS".to_string(),
            font,
            color: BLACK,
        },
        DrawCommand::PopClip,
    ] {
        list.push(command).unwrap();
    }
    list
}

fn hidpi_mixed() -> DrawList {
    let mut list = DrawList::new(16);
    let handle = list.add_image(checker()).unwrap();
    for command in [
        DrawCommand::FillRect {
            rect: rect(1, 1, 4, 3),
            color: BLACK,
        },
        DrawCommand::StrokeRect {
            rect: rect(0, 0, 6, 5),
            color: HALF_RED,
        },
        DrawCommand::PushClip {
            rect: rect(2, 0, 5, 4),
        },
        DrawCommand::FillRect {
            rect: rect(3, 2, 5, 5),
            color: BLACK,
        },
        DrawCommand::PopClip,
        DrawCommand::ImageBlit {
            image: handle,
            rect: rect(5, 4, 2, 2),
            opacity: 255,
        },
    ] {
        list.push(command).unwrap();
    }
    list
}

/// Phase-B additions. Image boxes that are not whole multiples of the source
/// size take the bicubic path on the CPU.
fn phase_b() -> Vec<Scenario> {
    let mut out = Vec::new();

    let mut bicubic = DrawList::new(8);
    let handle = bicubic.add_image(checker()).unwrap();
    bicubic
        .push(DrawCommand::ImageBlit {
            image: handle,
            rect: rect(0, 0, 3, 3),
            opacity: 255,
        })
        .unwrap();
    out.push(scenario("image_bicubic_1x", bicubic, 6, 5, 1));

    let mut bicubic_2x = DrawList::new(8);
    let handle = bicubic_2x.add_image(gradient()).unwrap();
    bicubic_2x
        .push(DrawCommand::ImageBlit {
            image: handle,
            rect: rect(1, 1, 5, 4),
            opacity: 200,
        })
        .unwrap();
    out.push(scenario("image_gradient_bicubic_2x", bicubic_2x, 8, 7, 2));

    let mut upscale = DrawList::new(8);
    let handle = upscale.add_image(gradient()).unwrap();
    upscale
        .push(DrawCommand::ImageBlit {
            image: handle,
            rect: rect(0, 0, 5, 4),
            opacity: 255,
        })
        .unwrap();
    out.push(scenario("image_gradient_upscale_1x", upscale, 6, 5, 1));
    out
}
