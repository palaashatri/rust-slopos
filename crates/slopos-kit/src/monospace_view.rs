use crate::{
    theme::ThemeContext, AccessibilityNode, AccessibilityRole, LayoutConstraint, Rect, Size,
    Widget, WidgetState,
};

#[derive(Clone, Debug)]
pub struct MonospaceCell {
    pub ch: char,
    pub fg: [f32; 4],
    pub bg: [f32; 4],
}

impl Default for MonospaceCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: [1.0, 1.0, 1.0, 1.0],
            bg: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

pub struct MonospaceView {
    state: WidgetState,
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<MonospaceCell>,
    pub cell_width: f32,
    pub cell_height: f32,
}

impl MonospaceView {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            state: WidgetState::new(),
            cols,
            rows,
            cells: vec![MonospaceCell::default(); cols * rows],
            cell_width: 8.0,
            cell_height: 16.0,
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let mut next = vec![MonospaceCell::default(); cols * rows];
        for row in 0..rows.min(self.rows) {
            for col in 0..cols.min(self.cols) {
                next[row * cols + col] = self.cells[row * self.cols + col].clone();
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.cells = next;
    }

    pub fn set_cell(&mut self, col: usize, row: usize, cell: MonospaceCell) {
        if col < self.cols && row < self.rows {
            self.cells[row * self.cols + col] = cell;
        }
    }
}

impl Widget for MonospaceView {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let cols = ((constraint.max_width / self.cell_width).floor() as usize).max(1);
        let rows = ((constraint.max_height / self.cell_height).floor() as usize).max(1);
        if cols != self.cols || rows != self.rows {
            self.resize(cols, rows);
        }
        let size = constraint.clamp(Size::new(
            cols as f32 * self.cell_width,
            rows as f32 * self.cell_height,
        ));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::Unknown,
            "monospace view",
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
