//! Session management (stub)

use super::window::Window;

pub struct Session {
    id: usize,
    name: String,
    windows: Vec<Window>,
    active_window: usize,
}

impl Session {
    pub fn new(id: usize, name: String) -> Self {
        Self {
            id,
            name,
            windows: Vec::new(),
            active_window: 0,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn add_window(&mut self, window: Window) -> usize {
        let idx = self.windows.len();
        self.windows.push(window);
        idx
    }
}
