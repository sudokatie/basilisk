//! Window (tab) management (stub)

use super::layout::Layout;
use super::pane::Pane;

pub struct Window {
    id: usize,
    name: String,
    panes: Vec<Pane>,
    layout: Layout,
    active_pane: usize,
}

impl Window {
    pub fn new(id: usize, name: String, initial_pane: Pane) -> Self {
        Self {
            id,
            name,
            layout: Layout::leaf(initial_pane.id()),
            panes: vec![initial_pane],
            active_pane: 0,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
