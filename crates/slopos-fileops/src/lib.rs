//! File operations shared by SLOPOS Files and the desktop, so both move,
//! rename and trash items the same way and ask the same questions.
//!
//! - [`ops`]: transactional rename, new folder, copy, move and permanent
//!   delete (see the module docs for the safety rules).
//! - [`trash`]: freedesktop.org Trash: move to Trash, list, restore, empty.
//! - [`dialogs`]: the rename, overwrite and error dialogs built on the
//!   SLOPOS toolkit, with the button indices their callers match on.
//!
//! Nothing here touches the window system except the dialog builders, which
//! only construct toolkit controls.

pub mod dialogs;
pub mod ops;
pub mod trash;
