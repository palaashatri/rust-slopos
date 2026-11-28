//! The input method the X11 host drives. With the `ibus` feature it is the
//! IBus client; without it, a stub that reports the input method unavailable
//! and never takes keys.

pub(crate) use crate::input_method::Update;

#[cfg(feature = "ibus")]
pub(crate) use super::ibus::InputMethod;

#[cfg(not(feature = "ibus"))]
pub(crate) use stub::InputMethod;

#[cfg(not(feature = "ibus"))]
mod stub {
    use crate::input_method::{InputMethodStatus, Update};

    /// Reports itself unavailable once, then never takes keys.
    pub struct InputMethod {
        pending: Vec<Update>,
    }

    impl InputMethod {
        pub fn start(_wake: impl Fn() + Send + 'static) -> Self {
            let status = InputMethodStatus::Unavailable("built without the ibus feature".into());
            Self {
                pending: vec![Update::Status(status)],
            }
        }

        pub fn is_active(&self) -> bool {
            false
        }

        pub fn wants_keys(&self) -> bool {
            false
        }

        pub fn set_window_focus(&mut self, _focused: bool) {}

        pub fn set_text_input(&mut self, _wanted: bool) {}

        pub fn reset(&mut self) {}

        pub fn set_cursor_location(&mut self, _x: i32, _y: i32, _width: i32, _height: i32) {}

        pub fn process_key(&mut self, _keysym: u32, _keycode: u32, _state: u32) -> bool {
            false
        }

        pub fn drain(&mut self) -> Vec<Update> {
            std::mem::take(&mut self.pending)
        }
    }
}
