//! Optional GPU backend for draw lists (phase B, not implemented).
//!
//! No GPU code exists yet. This module is empty, the `gpu` cargo feature adds
//! no dependencies, and the default build never enables the feature. The CPU
//! [`RasterSurface`](crate::render::RasterSurface) stays the only renderer. A
//! backend may land here only after it reproduces the CPU parity suite in
//! `render::list`: reference pixels, alpha, text, clipping and scale checks.
