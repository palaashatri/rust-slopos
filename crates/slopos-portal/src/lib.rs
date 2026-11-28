//! File chooser portal backend: the `org.freedesktop.impl.portal.FileChooser`
//! service that SLOPOS registers as `org.freedesktop.impl.portal.desktop.slopos`,
//! and the pure rules behind it.
//!
//! xdg-desktop-portal (the frontend) forwards applications' file chooser calls
//! to this backend when `portals.conf` selects it for `XDG_CURRENT_DESKTOP=SLOPOS`.
//!
//! The layers are:
//! - [`spec`]: the chooser request as the dialog program receives it (JSON),
//!   filters and the reply shape. Pure and serialisable.
//! - [`request`]: the portal's `a{sv}` options mapped onto a [`spec::ChooserSpec`].
//!   Pure apart from the zvariant types.
//! - [`response`]: the reply mapped onto the portal's response codes and `uris`.
//! - [`backend`]: runs a chooser: the dialog program in production, or a
//!   test backend that picks the first file.
//! - [`service`]: the D-Bus backend objects (`FileChooser` and `Request`).

pub mod backend;
pub mod request;
pub mod response;
pub mod service;
pub mod spec;

pub use spec::{ChooserReply, ChooserSpec, FilterSpec, Kind, Pattern};
