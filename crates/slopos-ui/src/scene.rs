//! Backend-neutral, bounded 2D display lists.
//!
//! This API owns no window, event loop, GPU, system font, or platform handle.
//! The SLOPOS software renderer is the initial reference consumer; other
//! renderers must replay commands in order with identical clipping semantics.

use crate::foundation::{color::Color, geometry::Rect};

/// The intentionally small M0 surface. Text, paths, clips, transforms and
/// resources need separate versioned contracts before being added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Clear(Color),
    FillRect { rect: Rect, color: Color },
    StrokeRect { rect: Rect, color: Color },
}

/// A renderer receives commands in submission order.
pub trait Renderer {
    type Error;

    fn draw(&mut self, command: Command) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneError {
    CommandLimitExceeded,
    AllocationFailed,
}

impl std::fmt::Display for SceneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandLimitExceeded => write!(formatter, "scene command limit exceeded"),
            Self::AllocationFailed => write!(formatter, "scene command allocation failed"),
        }
    }
}

impl std::error::Error for SceneError {}

/// A recording with a caller-specified hard command budget. A new scene is
/// normally recorded per invalidated frame, not on an idle polling loop.
#[derive(Debug, Clone)]
pub struct Scene {
    commands: Vec<Command>,
    command_limit: usize,
}

impl Scene {
    pub fn new(command_limit: usize) -> Self {
        Self {
            commands: Vec::new(),
            command_limit,
        }
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn clear_commands(&mut self) {
        self.commands.clear();
    }

    pub fn push(&mut self, command: Command) -> Result<(), SceneError> {
        if self.commands.len() >= self.command_limit {
            return Err(SceneError::CommandLimitExceeded);
        }
        self.commands
            .try_reserve(1)
            .map_err(|_| SceneError::AllocationFailed)?;
        self.commands.push(command);
        Ok(())
    }

    pub fn replay<R: Renderer>(&self, renderer: &mut R) -> Result<(), R::Error> {
        for &command in &self.commands {
            renderer.draw(command)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingRenderer(Vec<Command>);

    impl Renderer for RecordingRenderer {
        type Error = ();

        fn draw(&mut self, command: Command) -> Result<(), Self::Error> {
            self.0.push(command);
            Ok(())
        }
    }

    #[test]
    fn replay_preserves_painter_order() {
        let mut scene = Scene::new(3);
        let rect = Rect {
            x: -1,
            y: 2,
            width: 8,
            height: 9,
        };
        let expected = [
            Command::Clear(Color::WHITE),
            Command::FillRect {
                rect,
                color: Color::BLACK,
            },
            Command::StrokeRect {
                rect,
                color: Color::WHITE,
            },
        ];
        for command in expected {
            scene.push(command).unwrap();
        }
        let mut renderer = RecordingRenderer::default();
        scene.replay(&mut renderer).unwrap();
        assert_eq!(renderer.0, expected);
        assert_eq!(scene.len(), expected.len());
    }

    #[test]
    fn budget_is_enforced_without_mutating_recording() {
        let mut scene = Scene::new(1);
        scene.push(Command::Clear(Color::WHITE)).unwrap();
        assert_eq!(
            scene.push(Command::Clear(Color::BLACK)),
            Err(SceneError::CommandLimitExceeded)
        );
        assert_eq!(scene.len(), 1);
        scene.clear_commands();
        assert!(scene.is_empty());
        scene.push(Command::Clear(Color::BLACK)).unwrap();
    }

    #[test]
    fn zero_budget_rejects_first_command() {
        let mut scene = Scene::new(0);
        assert_eq!(
            scene.push(Command::Clear(Color::TRANSPARENT)),
            Err(SceneError::CommandLimitExceeded)
        );
    }
}
