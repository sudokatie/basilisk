//! Pane layout algorithms (stub)

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug)]
pub enum Layout {
    Leaf(usize),
    Horizontal(Vec<(f32, Layout)>),
    Vertical(Vec<(f32, Layout)>),
}

impl Layout {
    pub fn leaf(pane_id: usize) -> Self {
        Layout::Leaf(pane_id)
    }
}
