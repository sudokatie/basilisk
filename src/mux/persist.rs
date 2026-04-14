//! Session persistence - save and restore terminal state.
//!
//! Handles serialization of scrollback, session state, and
//! integration with external multiplexers (tmux/screen).

use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use crate::error::{Error, Result};
use crate::term::grid::Grid;

/// Serializable scrollback line with basic formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollbackLine {
    /// Plain text content.
    pub text: String,
    /// Whether line has formatting (we store raw text only for now).
    pub has_formatting: bool,
}

/// Pane scrollback state for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneScrollback {
    /// Pane ID.
    pub pane_id: u32,
    /// Scrollback lines (most recent last).
    pub lines: Vec<ScrollbackLine>,
    /// Cursor row position.
    pub cursor_row: u16,
    /// Cursor column position.
    pub cursor_col: u16,
}

impl PaneScrollback {
    /// Create from a grid, extracting scrollback as plain text.
    pub fn from_grid(pane_id: u32, grid: &Grid) -> Self {
        let mut lines = Vec::new();
        
        // Get scrollback lines
        for i in 0..grid.scrollback_len() {
            if let Some(row) = grid.scrollback_row(i) {
                // Extract text from row cells
                let text: String = row.cells.iter().map(|c| c.c).collect();
                lines.push(ScrollbackLine {
                    text: text.trim_end().to_string(),
                    has_formatting: true,
                });
            }
        }
        
        // Get visible grid lines
        for row_idx in 0..grid.lines() {
            let mut text = String::new();
            for col in 0..grid.cols() {
                let cell = grid.cell(col, row_idx);
                text.push(cell.c);
            }
            lines.push(ScrollbackLine {
                text: text.trim_end().to_string(),
                has_formatting: true,
            });
        }
        
        // Note: cursor position would need to come from Terminal, not Grid
        // For now, store defaults
        Self {
            pane_id,
            lines,
            cursor_row: 0,
            cursor_col: 0,
        }
    }
    
    /// Get total line count.
    pub fn len(&self) -> usize {
        self.lines.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Extended session state with scrollback.
#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedSession {
    /// Basic session metadata.
    pub id: u32,
    pub name: String,
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
    /// Window info.
    pub windows: Vec<PersistedWindow>,
    pub active_window: usize,
    /// Creation timestamp.
    pub created_at: u64,
    /// Last save timestamp.
    pub saved_at: u64,
}

/// Persisted window state.
#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedWindow {
    /// Window name.
    pub name: String,
    /// Pane scrollbacks.
    pub panes: Vec<PaneScrollback>,
    /// Active pane index.
    pub active_pane: usize,
}

impl PersistedSession {
    /// Get the persistence directory.
    pub fn persist_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("basilisk")
            .join("sessions")
    }

    /// Get path for a session file.
    pub fn path_for(session_id: u32) -> PathBuf {
        Self::persist_dir().join(format!("{}.json", session_id))
    }

    /// Save to disk.
    pub fn save(&self) -> Result<()> {
        let dir = Self::persist_dir();
        std::fs::create_dir_all(&dir)?;

        let path = Self::path_for(self.id);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("serialize error: {}", e)))?;
        
        std::fs::write(&path, json)?;
        Ok(())
    }

    /// Load from disk.
    pub fn load(session_id: u32) -> Result<Self> {
        let path = Self::path_for(session_id);
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content)
            .map_err(|e| Error::Config(format!("deserialize error: {}", e)))
    }

    /// List all saved sessions.
    pub fn list_all() -> Vec<Self> {
        let dir = Self::persist_dir();
        let mut sessions = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<Self>(&content) {
                            sessions.push(session);
                        }
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
        sessions
    }

    /// Delete saved session.
    pub fn delete(session_id: u32) -> Result<()> {
        let path = Self::path_for(session_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

/// External multiplexer detection (tmux/screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalMux {
    /// tmux session detected.
    Tmux,
    /// GNU screen detected.
    Screen,
    /// No external multiplexer.
    None,
}

impl ExternalMux {
    /// Detect current multiplexer from environment.
    pub fn detect() -> Self {
        if std::env::var("TMUX").is_ok() {
            Self::Tmux
        } else if std::env::var("STY").is_ok() {
            Self::Screen
        } else {
            Self::None
        }
    }

    /// Get the multiplexer name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Tmux => "tmux",
            Self::Screen => "screen",
            Self::None => "none",
        }
    }

    /// Check if running under a multiplexer.
    pub fn is_active(&self) -> bool {
        *self != Self::None
    }

    /// Get command to list sessions.
    pub fn list_command(&self) -> Option<&'static str> {
        match self {
            Self::Tmux => Some("tmux list-sessions"),
            Self::Screen => Some("screen -ls"),
            Self::None => None,
        }
    }

    /// Get command to attach to a session.
    pub fn attach_command(&self, session: &str) -> Option<String> {
        match self {
            Self::Tmux => Some(format!("tmux attach -t {}", session)),
            Self::Screen => Some(format!("screen -r {}", session)),
            Self::None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_scrollback_line() {
        let line = ScrollbackLine {
            text: "hello world".to_string(),
            has_formatting: false,
        };
        assert_eq!(line.text, "hello world");
    }

    #[test]
    fn test_pane_scrollback_empty() {
        let sb = PaneScrollback {
            pane_id: 1,
            lines: vec![],
            cursor_row: 0,
            cursor_col: 0,
        };
        assert!(sb.is_empty());
        assert_eq!(sb.len(), 0);
    }

    #[test]
    fn test_pane_scrollback_with_lines() {
        let sb = PaneScrollback {
            pane_id: 1,
            lines: vec![
                ScrollbackLine { text: "line 1".into(), has_formatting: false },
                ScrollbackLine { text: "line 2".into(), has_formatting: false },
            ],
            cursor_row: 1,
            cursor_col: 5,
        };
        assert!(!sb.is_empty());
        assert_eq!(sb.len(), 2);
    }

    #[test]
    fn test_external_mux_detect_none() {
        // In test environment, usually no multiplexer
        // This may fail if running inside tmux/screen
        let mux = ExternalMux::detect();
        // Just test that it doesn't panic
        let _ = mux.name();
        let _ = mux.is_active();
    }

    #[test]
    fn test_external_mux_tmux() {
        let mux = ExternalMux::Tmux;
        assert_eq!(mux.name(), "tmux");
        assert!(mux.is_active());
        assert!(mux.list_command().is_some());
        assert!(mux.attach_command("test").is_some());
    }

    #[test]
    fn test_external_mux_screen() {
        let mux = ExternalMux::Screen;
        assert_eq!(mux.name(), "screen");
        assert!(mux.is_active());
    }

    #[test]
    fn test_external_mux_none() {
        let mux = ExternalMux::None;
        assert_eq!(mux.name(), "none");
        assert!(!mux.is_active());
        assert!(mux.list_command().is_none());
        assert!(mux.attach_command("test").is_none());
    }

    #[test]
    fn test_persisted_session_serialization() {
        let session = PersistedSession {
            id: 1,
            name: "test".to_string(),
            shell: "/bin/bash".to_string(),
            cols: 80,
            rows: 24,
            windows: vec![PersistedWindow {
                name: "main".to_string(),
                panes: vec![],
                active_pane: 0,
            }],
            active_window: 0,
            created_at: 1234567890,
            saved_at: 1234567890,
        };

        let json = serde_json::to_string(&session).unwrap();
        let restored: PersistedSession = serde_json::from_str(&json).unwrap();
        
        assert_eq!(restored.id, 1);
        assert_eq!(restored.name, "test");
        assert_eq!(restored.windows.len(), 1);
    }

    #[test]
    fn test_persist_dir_creation() {
        let dir = PersistedSession::persist_dir();
        // Just verify it returns a path
        assert!(dir.to_string_lossy().contains("basilisk"));
    }
}
