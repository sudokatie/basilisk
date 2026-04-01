//! Main application - ties together session, renderer, and event handling

use std::sync::Arc;
use crate::mux::{Session, SessionId, SplitDirection};
use crate::render::{Event, KeyCode, Modifiers, WindowConfig};
use crate::input::KeyboardHandler;
use crate::config::Config;
use crate::Result;

/// Application state
pub struct App {
    session: Session,
    config: Config,
    should_quit: bool,
}

impl App {
    /// Create a new application with default config
    pub fn new() -> Result<Self> {
        let config = Config::default();
        Self::with_config(config)
    }

    /// Create application with config
    pub fn with_config(config: Config) -> Result<Self> {
        let mut session = Session::with_window(
            SessionId::new(1),
            "main".into(),
            config.terminal.cols,
            config.terminal.rows,
        );

        // Set shell from config
        session.set_shell(&config.terminal.shell);

        Ok(Self {
            session,
            config,
            should_quit: false,
        })
    }

    /// Get the session
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Get mutable session
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Get the config
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Check if app should quit
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Request quit
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Handle a window event
    pub fn handle_event(&mut self, event: Event) -> Result<bool> {
        match event {
            Event::CloseRequested => {
                self.should_quit = true;
                Ok(false)
            }

            Event::Resize { width, height } => {
                // Calculate new terminal dimensions
                let cell_width = self.config.font.size as u32 / 2; // Approximate
                let cell_height = self.config.font.size as u32;

                let cols = (width / cell_width).max(1) as u16;
                let rows = (height / cell_height).max(1) as u16;

                self.session.resize(cols, rows)?;
                Ok(true)
            }

            Event::Char(c) => {
                self.handle_char(c)?;
                Ok(true)
            }

            Event::Key { key, modifiers, pressed } => {
                if pressed {
                    self.handle_key(key, modifiers)?;
                }
                Ok(true)
            }

            Event::Redraw => {
                Ok(true)
            }
        }
    }

    /// Handle character input
    fn handle_char(&mut self, c: char) -> Result<()> {
        if let Some(pane) = self.session.active_pane_mut() {
            let bytes = pane.keyboard().char_to_bytes(c, &Modifiers::default());
            pane.write(&bytes)?;
        }
        Ok(())
    }

    /// Handle key press
    fn handle_key(&mut self, key: KeyCode, modifiers: Modifiers) -> Result<()> {
        // Check for app-level shortcuts first
        if modifiers.ctrl && modifiers.shift {
            match key {
                // Ctrl+Shift+N: New window
                KeyCode::Character('N') | KeyCode::Character('n') => {
                    self.session.create_window_with_shell("new".into())?;
                    return Ok(());
                }
                // Ctrl+Shift+W: Close window
                KeyCode::Character('W') | KeyCode::Character('w') => {
                    if let Some(window) = self.session.active_window() {
                        let id = window.id();
                        self.session.close_window(id);
                    }
                    if self.session.is_empty() {
                        self.should_quit = true;
                    }
                    return Ok(());
                }
                // Ctrl+Shift+H: Split horizontal
                KeyCode::Character('H') | KeyCode::Character('h') => {
                    self.session.split_pane_with_shell(SplitDirection::Horizontal)?;
                    return Ok(());
                }
                // Ctrl+Shift+V: Split vertical
                KeyCode::Character('V') | KeyCode::Character('v') => {
                    self.session.split_pane_with_shell(SplitDirection::Vertical)?;
                    return Ok(());
                }
                // Ctrl+Shift+Tab: Next window
                KeyCode::Tab => {
                    self.session.focus_next_window();
                    return Ok(());
                }
                _ => {}
            }
        }

        // Ctrl+Tab: Next pane
        if modifiers.ctrl && !modifiers.shift && key == KeyCode::Tab {
            if let Some(window) = self.session.active_window_mut() {
                window.focus_next();
            }
            return Ok(());
        }

        // Pass key to active pane
        if let Some(pane) = self.session.active_pane_mut() {
            if let Some(bytes) = pane.keyboard().key_to_bytes(key, &modifiers) {
                pane.write(&bytes)?;
            }
        }
        Ok(())
    }

    /// Process PTY output for all panes
    pub fn process(&mut self) -> Result<()> {
        self.session.process_all()
    }

    /// Spawn shells in all panes that don't have one
    pub fn spawn_shells(&mut self) -> Result<()> {
        let shell = self.config.terminal.shell.clone();

        for window in self.session.windows_mut() {
            for pane in window.panes_mut() {
                if pane.pty_fd().is_none() && !pane.has_exited() {
                    pane.spawn(&shell)?;
                }
            }
        }
        Ok(())
    }

    /// Get window config for the renderer
    pub fn window_config(&self) -> WindowConfig {
        WindowConfig {
            title: format!("Basilisk - {}", self.session.name()),
            width: self.config.window.width,
            height: self.config.window.height,
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new().expect("Failed to create default app")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_new() {
        let app = App::new().unwrap();
        assert!(!app.should_quit());
        assert_eq!(app.session().window_count(), 1);
    }

    #[test]
    fn app_quit() {
        let mut app = App::new().unwrap();
        assert!(!app.should_quit());
        app.quit();
        assert!(app.should_quit());
    }

    #[test]
    fn app_handle_close() {
        let mut app = App::new().unwrap();
        let result = app.handle_event(Event::CloseRequested).unwrap();
        assert!(!result);
        assert!(app.should_quit());
    }

    #[test]
    fn app_handle_resize() {
        let mut app = App::new().unwrap();
        let result = app.handle_event(Event::Resize { width: 1024, height: 768 }).unwrap();
        assert!(result);
    }

    #[test]
    fn app_handle_char() {
        let mut app = App::new().unwrap();
        // Without a spawned shell, this won't write anywhere but shouldn't crash
        let result = app.handle_event(Event::Char('a')).unwrap();
        assert!(result);
    }

    #[test]
    fn app_window_config() {
        let app = App::new().unwrap();
        let config = app.window_config();
        assert!(config.title.contains("Basilisk"));
    }

    #[test]
    fn app_with_config() {
        let mut config = Config::default();
        config.terminal.shell = "/bin/zsh".into();

        let app = App::with_config(config).unwrap();
        assert_eq!(app.session().shell(), "/bin/zsh");
    }
}
