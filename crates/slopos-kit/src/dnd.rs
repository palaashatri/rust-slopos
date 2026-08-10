use crate::Point;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum DragData {
    Files(Vec<PathBuf>),
    Text(String),
    Custom(Vec<u8>),
}

pub struct DragSession {
    pub payload: DragData,
    pub current_position: Point,
}

pub trait DragSource {
    fn drag_started(&self) -> DragSession;
    fn drag_ended(&self, session: DragSession, success: bool);
}

pub trait DropTarget {
    fn drag_entered(&mut self, session: &DragSession);
    fn drag_updated(&mut self, session: &DragSession);
    fn drag_exited(&mut self, session: &DragSession);
    fn perform_drop(&mut self, session: &DragSession) -> bool;
}
