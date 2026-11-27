//! What the user chooses in the Screenshot window. Plain code, no X11.

/// Which part of the screen to capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Screen,
    Window,
    Region,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Screen, Mode::Window, Mode::Region];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Screen => "Screen",
            Mode::Window => "Window",
            Mode::Region => "Region",
        }
    }

    pub fn accessible_name(self) -> &'static str {
        match self {
            Mode::Screen => "Capture the whole screen",
            Mode::Window => "Capture a window",
            Mode::Region => "Capture a dragged region",
        }
    }
}

/// How long to wait after the user presses Take screenshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delay {
    Immediate,
    Three,
    Ten,
}

impl Delay {
    pub const ALL: [Delay; 3] = [Delay::Immediate, Delay::Three, Delay::Ten];

    pub fn seconds(self) -> u32 {
        match self {
            Delay::Immediate => 0,
            Delay::Three => 3,
            Delay::Ten => 10,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Delay::Immediate => "No delay",
            Delay::Three => "3 seconds",
            Delay::Ten => "10 seconds",
        }
    }

    pub fn accessible_name(self) -> &'static str {
        match self {
            Delay::Immediate => "Take the screenshot without a delay",
            Delay::Three => "Take the screenshot after 3 seconds",
            Delay::Ten => "Take the screenshot after 10 seconds",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_have_the_seconds_the_user_chose() {
        let seconds: Vec<u32> = Delay::ALL.iter().map(|delay| delay.seconds()).collect();
        assert_eq!(seconds, vec![0, 3, 10]);
    }

    #[test]
    fn every_choice_has_a_label_and_an_accessible_name() {
        for mode in Mode::ALL {
            assert!(!mode.label().is_empty());
            assert!(!mode.accessible_name().is_empty());
        }
        for delay in Delay::ALL {
            assert!(!delay.label().is_empty());
            assert!(!delay.accessible_name().is_empty());
        }
    }
}
