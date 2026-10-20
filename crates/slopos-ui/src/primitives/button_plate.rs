use crate::foundation::{
    color::Color,
    geometry::{Rect, Size},
};
use crate::render::{RasterSurface, RenderError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonPlateVariant {
    Regular,
    Default,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonPlateState {
    Rest,
    Pressed,
    Disabled,
}

impl ButtonPlateVariant {
    pub const fn dimensions(self) -> Size {
        match self {
            Self::Regular => Size {
                width: 80,
                height: 20,
            },
            Self::Default => Size {
                width: 88,
                height: 28,
            },
            Self::Secondary => Size {
                width: 80,
                height: 16,
            },
        }
    }
}

pub fn paint_button_plate(
    surface: &RasterSurface,
    origin_x: i32,
    origin_y: i32,
    variant: ButtonPlateVariant,
    state: ButtonPlateState,
) -> Result<(), RenderError> {
    let dimensions = variant.dimensions();
    let (width, height, corner_depth) = match variant {
        ButtonPlateVariant::Regular => (dimensions.width, dimensions.height, 3),
        ButtonPlateVariant::Default => (dimensions.width, dimensions.height, 6),
        ButtonPlateVariant::Secondary => (dimensions.width, dimensions.height, 3),
    };
    let border_color = if state == ButtonPlateState::Disabled {
        Color {
            red: 0xc0,
            green: 0xc0,
            blue: 0xc0,
            alpha: 0xff,
        }
    } else {
        Color::BLACK
    };

    fill_stair_shape(
        surface,
        Rect {
            x: origin_x,
            y: origin_y,
            width,
            height,
        },
        corner_depth,
        border_color,
    )?;

    match variant {
        ButtonPlateVariant::Regular | ButtonPlateVariant::Secondary => {
            if state != ButtonPlateState::Pressed {
                fill_inset_shape(
                    surface,
                    Rect {
                        x: origin_x,
                        y: origin_y,
                        width,
                        height,
                    },
                    1,
                    corner_depth.saturating_sub(1),
                    Color::WHITE,
                )?;
            }
        }
        ButtonPlateVariant::Default => {
            fill_inset_shape(
                surface,
                Rect {
                    x: origin_x,
                    y: origin_y,
                    width,
                    height,
                },
                3,
                3,
                Color::WHITE,
            )?;
            fill_inset_shape(
                surface,
                Rect {
                    x: origin_x,
                    y: origin_y,
                    width,
                    height,
                },
                4,
                3,
                Color::BLACK,
            )?;
            if state != ButtonPlateState::Pressed {
                fill_inset_shape(
                    surface,
                    Rect {
                        x: origin_x,
                        y: origin_y,
                        width,
                        height,
                    },
                    5,
                    2,
                    Color::WHITE,
                )?;
            }
        }
    }

    Ok(())
}

fn fill_inset_shape(
    surface: &RasterSurface,
    shape: Rect,
    inset: u32,
    corner_depth: u32,
    color: Color,
) -> Result<(), RenderError> {
    let inset_twice = inset.checked_mul(2).ok_or(RenderError::GeometryOverflow)?;
    let width = shape
        .width
        .checked_sub(inset_twice)
        .ok_or(RenderError::GeometryOverflow)?;
    let height = shape
        .height
        .checked_sub(inset_twice)
        .ok_or(RenderError::GeometryOverflow)?;
    let inset = i32::try_from(inset).map_err(|_| RenderError::GeometryOverflow)?;
    fill_stair_shape(
        surface,
        Rect {
            x: shape
                .x
                .checked_add(inset)
                .ok_or(RenderError::GeometryOverflow)?,
            y: shape
                .y
                .checked_add(inset)
                .ok_or(RenderError::GeometryOverflow)?,
            width,
            height,
        },
        corner_depth,
        color,
    )
}

fn fill_stair_shape(
    surface: &RasterSurface,
    shape: Rect,
    corner_depth: u32,
    color: Color,
) -> Result<(), RenderError> {
    for row in 0..shape.height {
        let bottom = shape.height - 1 - row;
        let inset = corner_depth
            .saturating_sub(row)
            .max(corner_depth.saturating_sub(bottom));
        let inset_twice = inset.checked_mul(2).ok_or(RenderError::GeometryOverflow)?;
        let row_width = shape
            .width
            .checked_sub(inset_twice)
            .ok_or(RenderError::GeometryOverflow)?;
        let inset = i32::try_from(inset).map_err(|_| RenderError::GeometryOverflow)?;
        surface.fill_rect(
            Rect {
                x: shape
                    .x
                    .checked_add(inset)
                    .ok_or(RenderError::GeometryOverflow)?,
                y: shape
                    .y
                    .checked_add(i32::try_from(row).map_err(|_| RenderError::GeometryOverflow)?)
                    .ok_or(RenderError::GeometryOverflow)?,
                width: row_width,
                height: 1,
            },
            color,
        )?;
    }
    Ok(())
}
