//! Terminal pane (stub)

pub struct Pane {
    id: usize,
}

impl Pane {
    pub fn new(id: usize) -> Self {
        Self { id }
    }

    pub fn id(&self) -> usize {
        self.id
    }
}
