//! Native and delegated settings panels.

pub mod appearance;
pub mod datetime;
pub mod desktop;
pub mod network;
pub mod sound;

pub use appearance::show_appearance_dialog;
pub use datetime::show_datetime_dialog;
pub use desktop::show_wallpaper_dialog;
pub use network::show_network_dialog;
pub use sound::show_sound_dialog;
