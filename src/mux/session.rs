//! Session - a collection of windows

use super::window::{Window, WindowId, SplitDirection};
use super::pane::{Pane, PaneId};
use crate::Result;

/// Unique identifier for a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub u32);

impl SessionId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session:{}", self.0)
    }
}

/// A session contains multiple windows (tabs)
pub struct Session {
    id: SessionId,
    name: String,
    windows: Vec<Window>,
    active_window_idx: usize,
    next_pane_id: u32,
    next_window_id: u32,
    default_shell: String,
    default_cols: u16,
    default_rows: u16,
    scrollback: usize,
}

impl Session {
    /// Create a new session with default settings
    pub fn new(id: SessionId, name: String) -> Self {
        Self {
            id,
            name,
            windows: Vec::new(),
            active_window_idx: 0,
            next_pane_id: 1,
            next_window_id: 1,
            default_shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
            default_cols: 80,
            default_rows: 24,
            scrollback: 10000,
        }
    }

    /// Create session with initial window
    pub fn with_window(id: SessionId, name: String, cols: u16, rows: u16) -> Self {
        let mut session = Self::new(id, name);
        session.default_cols = cols;
        session.default_rows = rows;
        session.create_window("main".into());
        session
    }

    /// Get session ID
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Get session name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set session name
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Get all windows
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// Get mutable windows
    pub fn windows_mut(&mut self) -> &mut [Window] {
        &mut self.windows
    }

    /// Get window count
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Get active window
    pub fn active_window(&self) -> Option<&Window> {
        self.windows.get(self.active_window_idx)
    }

    /// Get mutable active window
    pub fn active_window_mut(&mut self) -> Option<&mut Window> {
        self.windows.get_mut(self.active_window_idx)
    }

    /// Get active window index
    pub fn active_window_index(&self) -> usize {
        self.active_window_idx
    }

    /// Set active window by index
    pub fn set_active_window(&mut self, index: usize) -> bool {
        if index < self.windows.len() {
            self.active_window_idx = index;
            true
        } else {
            false
        }
    }

    /// Set active window by ID
    pub fn set_active_window_by_id(&mut self, id: WindowId) -> bool {
        if let Some(idx) = self.windows.iter().position(|w| w.id() == id) {
            self.active_window_idx = idx;
            true
        } else {
            false
        }
    }

    /// Get window by ID
    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| w.id() == id)
    }

    /// Get mutable window by ID
    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id() == id)
    }

    /// Create a new window with a pane
    pub fn create_window(&mut self, name: String) -> WindowId {
        let window_id = WindowId::new(self.next_window_id);
        self.next_window_id += 1;

        let pane_id = PaneId::new(self.next_pane_id);
        self.next_pane_id += 1;

        let pane = Pane::new(pane_id, self.default_cols, self.default_rows, self.scrollback);
        let window = Window::new(window_id, name, pane, self.default_cols, self.default_rows);

        self.windows.push(window);
        window_id
    }

    /// Create window and spawn shell
    pub fn create_window_with_shell(&mut self, name: String) -> Result<WindowId> {
        let window_id = self.create_window(name);
        let shell = self.default_shell.clone();

        // Spawn shell in the new window's pane
        if let Some(window) = self.window_mut(window_id) {
            if let Some(pane) = window.active_pane_mut() {
                pane.spawn(&shell)?;
            }
        }

        Ok(window_id)
    }

    /// Close a window
    pub fn close_window(&mut self, id: WindowId) -> bool {
        if let Some(idx) = self.windows.iter().position(|w| w.id() == id) {
            self.windows.remove(idx);

            // Adjust active window index
            if self.active_window_idx >= self.windows.len() && !self.windows.is_empty() {
                self.active_window_idx = self.windows.len() - 1;
            }
            true
        } else {
            false
        }
    }

    /// Focus next window
    pub fn focus_next_window(&mut self) {
        if !self.windows.is_empty() {
            self.active_window_idx = (self.active_window_idx + 1) % self.windows.len();
        }
    }

    /// Focus previous window
    pub fn focus_prev_window(&mut self) {
        if !self.windows.is_empty() {
            self.active_window_idx = if self.active_window_idx == 0 {
                self.windows.len() - 1
            } else {
                self.active_window_idx - 1
            };
        }
    }

    /// Split the active pane in the active window
    pub fn split_pane(&mut self, direction: SplitDirection) -> Option<PaneId> {
        let pane_id = PaneId::new(self.next_pane_id);
        self.next_pane_id += 1;

        let pane = Pane::new(pane_id, self.default_cols, self.default_rows, self.scrollback);

        if let Some(window) = self.active_window_mut() {
            window.add_pane(pane, direction);
            Some(pane_id)
        } else {
            None
        }
    }

    /// Split pane and spawn shell
    pub fn split_pane_with_shell(&mut self, direction: SplitDirection) -> Result<Option<PaneId>> {
        let shell = self.default_shell.clone();

        if let Some(pane_id) = self.split_pane(direction) {
            // Spawn shell in new pane
            if let Some(window) = self.active_window_mut() {
                if let Some(pane) = window.pane_mut(pane_id) {
                    pane.spawn(&shell)?;
                }
            }
            Ok(Some(pane_id))
        } else {
            Ok(None)
        }
    }

    /// Get active pane from active window
    pub fn active_pane(&self) -> Option<&Pane> {
        self.active_window().and_then(|w| w.active_pane())
    }

    /// Get mutable active pane
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.active_window_mut().and_then(|w| w.active_pane_mut())
    }

    /// Resize all windows
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.default_cols = cols;
        self.default_rows = rows;

        for window in &mut self.windows {
            window.resize(cols, rows)?;
        }
        Ok(())
    }

    /// Set default shell
    pub fn set_shell(&mut self, shell: &str) {
        self.default_shell = shell.to_string();
    }

    /// Get default shell
    pub fn shell(&self) -> &str {
        &self.default_shell
    }

    /// Process all panes (read from PTY and update terminals)
    pub fn process_all(&mut self) -> Result<()> {
        for window in &mut self.windows {
            for pane in window.panes_mut() {
                pane.read_and_process()?;
            }
        }
        Ok(())
    }

    /// Check if all windows are closed
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Find pane by ID across all windows
    pub fn find_pane(&self, pane_id: PaneId) -> Option<(&Window, &Pane)> {
        for window in &self.windows {
            if let Some(pane) = window.pane(pane_id) {
                return Some((window, pane));
            }
        }
        None
    }

    /// Find mutable pane by ID
    pub fn find_pane_mut(&mut self, pane_id: PaneId) -> Option<&mut Pane> {
        for window in &mut self.windows {
            if let Some(pane) = window.pane_mut(pane_id) {
                return Some(pane);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_display() {
        let id = SessionId::new(7);
        assert_eq!(format!("{}", id), "session:7");
    }

    #[test]
    fn session_new() {
        let session = Session::new(SessionId::new(1), "test".into());
        assert_eq!(session.id(), SessionId::new(1));
        assert_eq!(session.name(), "test");
        assert_eq!(session.window_count(), 0);
    }

    #[test]
    fn session_with_window() {
        let session = Session::with_window(SessionId::new(1), "main".into(), 80, 24);
        assert_eq!(session.window_count(), 1);
        assert!(session.active_window().is_some());
    }

    #[test]
    fn session_create_window() {
        let mut session = Session::new(SessionId::new(1), "test".into());
        let win_id = session.create_window("win1".into());

        assert_eq!(session.window_count(), 1);
        assert!(session.window(win_id).is_some());
    }

    #[test]
    fn session_close_window() {
        let mut session = Session::new(SessionId::new(1), "test".into());
        let win_id = session.create_window("win1".into());

        assert!(session.close_window(win_id));
        assert_eq!(session.window_count(), 0);
        assert!(session.is_empty());
    }

    #[test]
    fn session_focus_windows() {
        let mut session = Session::new(SessionId::new(1), "test".into());
        session.create_window("win1".into());
        session.create_window("win2".into());

        assert_eq!(session.active_window_index(), 0);

        session.focus_next_window();
        assert_eq!(session.active_window_index(), 1);

        session.focus_next_window();
        assert_eq!(session.active_window_index(), 0); // Wrap

        session.focus_prev_window();
        assert_eq!(session.active_window_index(), 1); // Wrap back
    }

    #[test]
    fn session_set_name() {
        let mut session = Session::new(SessionId::new(1), "old".into());
        session.set_name("new");
        assert_eq!(session.name(), "new");
    }

    #[test]
    fn session_split_pane() {
        let mut session = Session::with_window(SessionId::new(1), "test".into(), 80, 24);

        let pane_id = session.split_pane(SplitDirection::Horizontal);
        assert!(pane_id.is_some());

        let window = session.active_window().unwrap();
        assert_eq!(window.pane_count(), 2);
    }

    #[test]
    fn session_find_pane() {
        let mut session = Session::with_window(SessionId::new(1), "test".into(), 80, 24);

        let pane_id = session.active_pane().unwrap().id();
        let found = session.find_pane(pane_id);
        assert!(found.is_some());
    }

    #[test]
    fn session_shell() {
        let mut session = Session::new(SessionId::new(1), "test".into());
        session.set_shell("/bin/zsh");
        assert_eq!(session.shell(), "/bin/zsh");
    }
}
