//! The SLOPOS Volume Mixer: the channels for the sound server's outputs,
//! inputs and applications ([`model`]), the controls that show and change
//! them ([`view`]), and the connection that runs the changes ([`provider`]).
//!
//! The Volume Mixer app hosts the whole thing in a window; the top bar hosts
//! the same [`MixerView`] in its sound popover and gets its answers from its
//! own status provider. Both use [`model::apply`] for every change.

pub mod model;
pub mod provider;
pub mod view;

pub use model::{apply, Channel, Command, Target, MAX_APPLICATIONS};
pub use view::{MixerView, Outcome};
