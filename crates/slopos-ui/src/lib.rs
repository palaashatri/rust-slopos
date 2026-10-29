//! SLOPOS UI toolkit.
//!
//! SLOPOS draws its own controls: geometry, colour, text and state come from
//! this crate, not from a third-party toolkit's theme. The pieces are:
//!
//! - [`foundation`] and [`layout`]: integer geometry, insets and layout.
//! - [`render`], [`text`], [`image`], [`icons`]: pixel-exact software
//!   rendering (tiny-skia), shaped text (cosmic-text) and freedesktop icons.
//! - [`primitives`] and [`controls`]: Classic component painters and
//!   interactive controls with explicit state machines.
//! - [`event`] and [`platform`]: input events and the X11 window host, which
//!   also exposes each app's AccessKit tree over AT-SPI (feature `atspi`).
pub mod animation;
pub mod controls;
pub mod event;
pub mod foundation;
pub mod icons;
pub mod image;
pub mod image_viewport;
pub mod layout;
pub mod platform;
pub mod primitives;
pub mod render;
pub mod scene;
pub mod scene_raster;
pub mod text;
