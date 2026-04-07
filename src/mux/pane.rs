//! Pane - a terminal instance within a window

use crate::term::{Terminal, ColorPalette};
use crate::pty::Pty;
use crate::input::KeyboardHandler;
use crate::Result;

/// Unique identifier for a pane
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u32);

impl PaneId {
    /// Create a new pane ID
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for PaneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pane:{}", self.0)
    }
}

/// A pane contains a terminal and PTY
pub struct Pane {
    id: PaneId,
    terminal: Terminal,
    pty: Option<Pty>,
    keyboard: KeyboardHandler,
    title: String,
    cols: u16,
    rows: u16,
    exited: bool,
    exit_status: Option<i32>,
}

impl Pane {
    /// Create a new pane with the given dimensions
    pub fn new(id: PaneId, cols: u16, rows: u16, scrollback: usize) -> Self {
        Self {
            id,
            terminal: Terminal::new(cols, rows, scrollback),
            pty: None,
            keyboard: KeyboardHandler::new(),
            title: String::new(),
            cols,
            rows,
            exited: false,
            exit_status: None,
        }
    }

    /// Spawn a shell in this pane
    pub fn spawn(&mut self, shell: &str) -> Result<()> {
        let pty = Pty::spawn(shell, self.cols, self.rows)?;
        self.pty = Some(pty);
        Ok(())
    }

    /// Get pane ID
    pub fn id(&self) -> PaneId {
        self.id
    }

    /// Get terminal reference
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// Get mutable terminal reference
    pub fn terminal_mut(&mut self) -> &mut Terminal {
        &mut self.terminal
    }

    /// Set the color palette for this pane's terminal
    pub fn set_color_palette(&mut self, palette: ColorPalette) {
        self.terminal.set_color_palette(palette);
    }

    /// Get keyboard handler reference
    pub fn keyboard(&self) -> &KeyboardHandler {
        &self.keyboard
    }

    /// Get mutable keyboard handler reference
    pub fn keyboard_mut(&mut self) -> &mut KeyboardHandler {
        &mut self.keyboard
    }

    /// Get pane dimensions
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Resize the pane
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.cols = cols;
        self.rows = rows;
        self.terminal.resize(cols, rows);

        if let Some(pty) = &self.pty {
            pty.resize(cols, rows)?;
        }
        Ok(())
    }

    /// Get the pane title (from terminal or custom)
    pub fn title(&self) -> &str {
        if self.title.is_empty() {
            self.terminal.title()
        } else {
            &self.title
        }
    }

    /// Set a custom title
    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }

    /// Check if the pane has exited
    pub fn has_exited(&self) -> bool {
        self.exited
    }

    /// Get exit status if exited
    pub fn exit_status(&self) -> Option<i32> {
        self.exit_status
    }

    /// Mark pane as exited
    pub fn mark_exited(&mut self, status: i32) {
        self.exited = true;
        self.exit_status = Some(status);
    }

    /// Read from PTY and process output
    pub fn read_and_process(&mut self) -> Result<bool> {
        if self.pty.is_none() || self.exited {
            return Ok(false);
        }

        let mut buf = [0u8; 4096];
        let pty = self.pty.as_mut().unwrap();

        match pty.read(&mut buf) {
            Ok(0) => {
                // EOF - process exited
                self.mark_exited(0);
                Ok(false)
            }
            Ok(n) => {
                self.terminal.process(&buf[..n]);
                Ok(true)
            }
            Err(e) => {
                // Check if it's a WouldBlock error
                if let crate::Error::Io(ref io_err) = e {
                    if io_err.kind() == std::io::ErrorKind::WouldBlock {
                        return Ok(false);
                    }
                }
                Err(e)
            }
        }
    }

    /// Write bytes to the PTY
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        if let Some(pty) = &mut self.pty {
            pty.write(data)
        } else {
            Ok(0)
        }
    }

    /// Get PTY file descriptor for polling
    pub fn pty_fd(&self) -> Option<i32> {
        self.pty.as_ref().map(|p| p.master_fd())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_id_display() {
        let id = PaneId::new(42);
        assert_eq!(format!("{}", id), "pane:42");
    }

    #[test]
    fn pane_new() {
        let pane = Pane::new(PaneId::new(1), 80, 24, 1000);
        assert_eq!(pane.id(), PaneId::new(1));
        assert_eq!(pane.size(), (80, 24));
        assert!(!pane.has_exited());
    }

    #[test]
    fn pane_title_default() {
        let pane = Pane::new(PaneId::new(1), 80, 24, 1000);
        assert!(pane.title().is_empty());
    }

    #[test]
    fn pane_custom_title() {
        let mut pane = Pane::new(PaneId::new(1), 80, 24, 1000);
        pane.set_title("My Pane");
        assert_eq!(pane.title(), "My Pane");
    }

    #[test]
    fn pane_resize() {
        let mut pane = Pane::new(PaneId::new(1), 80, 24, 1000);
        pane.resize(120, 40).unwrap();
        assert_eq!(pane.size(), (120, 40));
    }

    #[test]
    fn pane_exit_status() {
        let mut pane = Pane::new(PaneId::new(1), 80, 24, 1000);
        assert!(pane.exit_status().is_none());

        pane.mark_exited(0);
        assert!(pane.has_exited());
        assert_eq!(pane.exit_status(), Some(0));
    }

    #[test]
    fn pane_no_pty() {
        let pane = Pane::new(PaneId::new(1), 80, 24, 1000);
        assert!(pane.pty_fd().is_none());
    }

    #[test]
    fn pane_write_no_pty() {
        let mut pane = Pane::new(PaneId::new(1), 80, 24, 1000);
        let result = pane.write(b"test");
        assert_eq!(result.unwrap(), 0);
    }
}
