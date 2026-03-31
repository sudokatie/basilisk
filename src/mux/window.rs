//! Window - a collection of panes with a layout

use super::layout::{Layout, Rect};
use super::pane::{Pane, PaneId};
use crate::Result;

/// Unique identifier for a window
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u32);

impl WindowId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "window:{}", self.0)
    }
}

/// A window contains multiple panes arranged in a layout
pub struct Window {
    id: WindowId,
    name: String,
    panes: Vec<Pane>,
    layout: Layout,
    active_pane_idx: usize,
    width: u16,
    height: u16,
}

impl Window {
    /// Create a new window with an initial pane
    pub fn new(id: WindowId, name: String, initial_pane: Pane, width: u16, height: u16) -> Self {
        let pane_id = initial_pane.id().0 as usize;
        Self {
            id,
            name,
            layout: Layout::leaf(pane_id),
            panes: vec![initial_pane],
            active_pane_idx: 0,
            width,
            height,
        }
    }

    /// Get window ID
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// Get window name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set window name
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Get window dimensions
    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Get all panes
    pub fn panes(&self) -> &[Pane] {
        &self.panes
    }

    /// Get mutable panes
    pub fn panes_mut(&mut self) -> &mut [Pane] {
        &mut self.panes
    }

    /// Get active pane
    pub fn active_pane(&self) -> Option<&Pane> {
        self.panes.get(self.active_pane_idx)
    }

    /// Get mutable active pane
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(self.active_pane_idx)
    }

    /// Get active pane index
    pub fn active_pane_index(&self) -> usize {
        self.active_pane_idx
    }

    /// Set active pane by index
    pub fn set_active_pane(&mut self, index: usize) -> bool {
        if index < self.panes.len() {
            self.active_pane_idx = index;
            true
        } else {
            false
        }
    }

    /// Set active pane by ID
    pub fn set_active_pane_by_id(&mut self, id: PaneId) -> bool {
        if let Some(idx) = self.panes.iter().position(|p| p.id() == id) {
            self.active_pane_idx = idx;
            true
        } else {
            false
        }
    }

    /// Get pane by ID
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id() == id)
    }

    /// Get mutable pane by ID
    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id() == id)
    }

    /// Add a pane to the window
    pub fn add_pane(&mut self, pane: Pane, direction: SplitDirection) {
        let new_pane_idx = pane.id().0 as usize;
        let active_idx = self.active_pane().map(|p| p.id().0 as usize).unwrap_or(0);

        // Update layout
        self.layout = self.layout.split(active_idx, new_pane_idx, direction);
        self.panes.push(pane);
    }

    /// Remove a pane from the window
    pub fn remove_pane(&mut self, id: PaneId) -> Option<Pane> {
        let idx = self.panes.iter().position(|p| p.id() == id)?;
        let pane = self.panes.remove(idx);

        // Update layout
        self.layout = self.layout.remove(id.0 as usize);

        // Adjust active pane index
        if self.active_pane_idx >= self.panes.len() && !self.panes.is_empty() {
            self.active_pane_idx = self.panes.len() - 1;
        }

        Some(pane)
    }

    /// Get number of panes
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Resize the window
    pub fn resize(&mut self, width: u16, height: u16) -> Result<()> {
        self.width = width;
        self.height = height;

        // Recalculate pane positions
        let bounds = Rect { x: 0, y: 0, width, height };
        let positions = self.layout.calculate_positions(&bounds);

        for (pane_id, rect) in positions {
            if let Some(pane) = self.panes.iter_mut().find(|p| p.id().0 as usize == pane_id) {
                pane.resize(rect.width, rect.height)?;
            }
        }

        Ok(())
    }

    /// Get calculated pane positions
    pub fn pane_positions(&self) -> Vec<(PaneId, Rect)> {
        let bounds = Rect { x: 0, y: 0, width: self.width, height: self.height };
        let positions = self.layout.calculate_positions(&bounds);

        positions
            .into_iter()
            .filter_map(|(id, rect)| {
                self.panes
                    .iter()
                    .find(|p| p.id().0 as usize == id)
                    .map(|p| (p.id(), rect))
            })
            .collect()
    }

    /// Focus next pane
    pub fn focus_next(&mut self) {
        if !self.panes.is_empty() {
            self.active_pane_idx = (self.active_pane_idx + 1) % self.panes.len();
        }
    }

    /// Focus previous pane
    pub fn focus_prev(&mut self) {
        if !self.panes.is_empty() {
            self.active_pane_idx = if self.active_pane_idx == 0 {
                self.panes.len() - 1
            } else {
                self.active_pane_idx - 1
            };
        }
    }
}

/// Direction for splitting a pane
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pane(id: u32) -> Pane {
        Pane::new(PaneId::new(id), 80, 24, 1000)
    }

    #[test]
    fn window_id_display() {
        let id = WindowId::new(5);
        assert_eq!(format!("{}", id), "window:5");
    }

    #[test]
    fn window_new() {
        let pane = make_pane(1);
        let window = Window::new(WindowId::new(1), "main".into(), pane, 80, 24);
        assert_eq!(window.id(), WindowId::new(1));
        assert_eq!(window.name(), "main");
        assert_eq!(window.pane_count(), 1);
    }

    #[test]
    fn window_active_pane() {
        let pane = make_pane(1);
        let window = Window::new(WindowId::new(1), "test".into(), pane, 80, 24);
        assert!(window.active_pane().is_some());
        assert_eq!(window.active_pane().unwrap().id(), PaneId::new(1));
    }

    #[test]
    fn window_set_name() {
        let pane = make_pane(1);
        let mut window = Window::new(WindowId::new(1), "old".into(), pane, 80, 24);
        window.set_name("new");
        assert_eq!(window.name(), "new");
    }

    #[test]
    fn window_focus_next() {
        let pane1 = make_pane(1);
        let pane2 = make_pane(2);
        let mut window = Window::new(WindowId::new(1), "test".into(), pane1, 80, 24);
        window.panes.push(pane2);

        assert_eq!(window.active_pane_index(), 0);
        window.focus_next();
        assert_eq!(window.active_pane_index(), 1);
        window.focus_next();
        assert_eq!(window.active_pane_index(), 0); // Wrap around
    }

    #[test]
    fn window_focus_prev() {
        let pane1 = make_pane(1);
        let pane2 = make_pane(2);
        let mut window = Window::new(WindowId::new(1), "test".into(), pane1, 80, 24);
        window.panes.push(pane2);

        assert_eq!(window.active_pane_index(), 0);
        window.focus_prev();
        assert_eq!(window.active_pane_index(), 1); // Wrap around
    }

    #[test]
    fn window_set_active_pane() {
        let pane1 = make_pane(1);
        let pane2 = make_pane(2);
        let mut window = Window::new(WindowId::new(1), "test".into(), pane1, 80, 24);
        window.panes.push(pane2);

        assert!(window.set_active_pane(1));
        assert_eq!(window.active_pane_index(), 1);
        assert!(!window.set_active_pane(5)); // Out of bounds
    }

    #[test]
    fn window_get_pane_by_id() {
        let pane = make_pane(42);
        let window = Window::new(WindowId::new(1), "test".into(), pane, 80, 24);

        assert!(window.pane(PaneId::new(42)).is_some());
        assert!(window.pane(PaneId::new(99)).is_none());
    }
}
