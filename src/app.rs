//! Application lifecycle and main event loop

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey, ModifiersState};
use winit::window::{Window, WindowId};

use crate::bell::{Bell, BellConfig};
use crate::config::Config;
use crate::input::{KeyCode, Modifiers, KeyboardHandler};
use crate::mux::{Session, SessionId, SplitDirection};
use crate::render::{Renderer, TextRenderer};
use crate::term::selection::{SelectionManager, SelectionType};
use crate::clipboard::Clipboard;
use crate::{Error, Result};

/// Terminal application state
pub struct App {
    config: Config,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    text_renderer: Option<TextRenderer>,
    session: Option<Session>,
    clipboard: Clipboard,
    selection: SelectionManager,
    bell: Bell,
    
    // Prefix mode state (for tmux-like keybindings)
    prefix_active: bool,
    prefix_time: Option<Instant>,
    
    // Modifiers state
    modifiers: ModifiersState,
    
    // Mouse state
    mouse_position: (f64, f64),
    mouse_pressed: bool,
    
    // Timing
    last_frame: Instant,
    frame_count: u64,
    
    // PTY communication
    pty_rx: Option<mpsc::Receiver<Vec<u8>>>,
    
    // Exit flag
    should_exit: bool,
    
    // Pane zoom state
    pane_zoomed: bool,
    
    // Copy mode state (vi-like navigation for selection)
    copy_mode: bool,
    copy_mode_cursor: (u16, u16), // (col, row)
}

impl App {
    /// Create a new application with the given config
    pub fn new(config: Config) -> Self {
        Self {
            config,
            window: None,
            renderer: None,
            text_renderer: None,
            session: None,
            clipboard: Clipboard::new(),
            selection: SelectionManager::new(),
            bell: Bell::default(),
            prefix_active: false,
            prefix_time: None,
            modifiers: ModifiersState::empty(),
            mouse_position: (0.0, 0.0),
            mouse_pressed: false,
            last_frame: Instant::now(),
            frame_count: 0,
            pty_rx: None,
            should_exit: false,
            pane_zoomed: false,
            copy_mode: false,
            copy_mode_cursor: (0, 0),
        }
    }

    /// Run the application
    pub fn run(config: Config) -> Result<()> {
        let event_loop = EventLoop::new()
            .map_err(|e| Error::Window(e.to_string()))?;
        
        event_loop.set_control_flow(ControlFlow::Poll);
        
        let mut app = App::new(config);
        
        event_loop.run_app(&mut app)
            .map_err(|e| Error::Window(e.to_string()))?;
        
        Ok(())
    }

    /// Initialize the terminal session
    fn init_session(&mut self) -> Result<()> {
        let cols = self.config.terminal.cols;
        let rows = self.config.terminal.rows;

        let mut session = Session::with_window(
            SessionId::new(1),
            "main".into(),
            cols,
            rows,
        );
        session.set_shell(&self.config.terminal.shell);

        // Build color palette from config and set it on the terminal
        let palette = self.config.colors.build_palette();

        // Spawn shell in the first pane and configure terminal
        if let Some(window) = session.active_window_mut() {
            if let Some(pane) = window.active_pane_mut() {
                pane.terminal_mut().set_color_palette(palette);
                pane.spawn(&self.config.terminal.shell)?;
            }
        }

        self.session = Some(session);
        Ok(())
    }

    /// Handle prefix key combinations (tmux-like)
    fn handle_prefix_key(&mut self, key: &Key) -> bool {
        let action = match key {
            Key::Character(c) => {
                match c.as_str() {
                    "c" => Some(PrefixAction::NewWindow),
                    "\"" => Some(PrefixAction::SplitHorizontal),
                    "%" => Some(PrefixAction::SplitVertical),
                    "n" => Some(PrefixAction::NextWindow),
                    "p" => Some(PrefixAction::PrevWindow),
                    "d" => Some(PrefixAction::Detach),
                    "x" => Some(PrefixAction::ClosePane),
                    "z" => Some(PrefixAction::ZoomPane),
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                        let idx = c.chars().next().unwrap().to_digit(10).unwrap() as usize;
                        Some(PrefixAction::SelectWindow(idx))
                    }
                    "[" => Some(PrefixAction::CopyMode),
                    _ => None,
                }
            }
            Key::Named(NamedKey::ArrowUp) => Some(PrefixAction::FocusPaneUp),
            Key::Named(NamedKey::ArrowDown) => Some(PrefixAction::FocusPaneDown),
            Key::Named(NamedKey::ArrowLeft) => Some(PrefixAction::FocusPaneLeft),
            Key::Named(NamedKey::ArrowRight) => Some(PrefixAction::FocusPaneRight),
            _ => None,
        };

        self.prefix_active = false;
        self.prefix_time = None;

        if let Some(action) = action {
            self.execute_prefix_action(action);
            true
        } else {
            false
        }
    }

    /// Execute a prefix action
    fn execute_prefix_action(&mut self, action: PrefixAction) {
        let Some(session) = &mut self.session else { return };

        match action {
            PrefixAction::NewWindow => {
                if let Ok(window_id) = session.create_window_with_shell("new".into()) {
                    session.set_active_window_by_id(window_id);
                }
            }
            PrefixAction::SplitHorizontal => {
                let _ = session.split_pane_with_shell(SplitDirection::Horizontal);
            }
            PrefixAction::SplitVertical => {
                let _ = session.split_pane_with_shell(SplitDirection::Vertical);
            }
            PrefixAction::NextWindow => {
                session.focus_next_window();
            }
            PrefixAction::PrevWindow => {
                session.focus_prev_window();
            }
            PrefixAction::SelectWindow(idx) => {
                session.set_active_window(idx);
            }
            PrefixAction::ClosePane => {
                if let Some(window) = session.active_window_mut() {
                    if let Some(pane) = window.active_pane() {
                        let pane_id = pane.id();
                        window.remove_pane(pane_id);
                    }
                }
            }
            PrefixAction::Detach => {
                // Save session state and exit
                self.should_exit = true;
            }
            PrefixAction::ZoomPane => {
                // Toggle zoom on active pane
                self.pane_zoomed = !self.pane_zoomed;
                // When zoomed, the render function will show only the active pane
            }
            PrefixAction::CopyMode => {
                // Enter copy mode for keyboard-based selection
                self.copy_mode = true;
                // Initialize cursor at current terminal cursor position
                if let Some(pane) = session.active_pane() {
                    let cursor = pane.terminal().cursor();
                    self.copy_mode_cursor = (cursor.col, cursor.line);
                    // Start selection at cursor
                    self.selection.start_normal(cursor.col, cursor.line);
                }
            }
            PrefixAction::FocusPaneUp |
            PrefixAction::FocusPaneDown |
            PrefixAction::FocusPaneLeft |
            PrefixAction::FocusPaneRight => {
                if let Some(window) = session.active_window_mut() {
                    match action {
                        PrefixAction::FocusPaneUp | PrefixAction::FocusPaneLeft => {
                            window.focus_prev();
                        }
                        _ => {
                            window.focus_next();
                        }
                    }
                }
            }
        }
    }

    /// Handle copy mode input (vi-like navigation)
    fn handle_copy_mode_key(&mut self, event: &KeyEvent) {
        let (col, row) = self.copy_mode_cursor;
        let (max_col, max_row) = self.session.as_ref()
            .and_then(|s| s.active_pane())
            .map(|p| (p.terminal().grid().cols().saturating_sub(1), 
                      p.terminal().grid().lines().saturating_sub(1)))
            .unwrap_or((79, 23));

        match &event.logical_key {
            // Vi navigation
            Key::Character(c) => match c.as_str() {
                "h" => self.copy_mode_cursor.0 = col.saturating_sub(1),
                "j" => self.copy_mode_cursor.1 = (row + 1).min(max_row),
                "k" => self.copy_mode_cursor.1 = row.saturating_sub(1),
                "l" => self.copy_mode_cursor.0 = (col + 1).min(max_col),
                "0" => self.copy_mode_cursor.0 = 0,
                "$" => self.copy_mode_cursor.0 = max_col,
                "g" => self.copy_mode_cursor.1 = 0,
                "G" => self.copy_mode_cursor.1 = max_row,
                "w" => {
                    // Move to next word (simplified: next non-space after space)
                    self.copy_mode_cursor.0 = (col + 5).min(max_col);
                }
                "b" => {
                    // Move to previous word
                    self.copy_mode_cursor.0 = col.saturating_sub(5);
                }
                "v" => {
                    // Start/toggle selection
                    self.selection.start_normal(self.copy_mode_cursor.0, self.copy_mode_cursor.1);
                }
                "y" => {
                    // Yank (copy) selection and exit copy mode
                    if let Some(session) = &self.session {
                        if let Some(pane) = session.active_pane() {
                            if let Some(text) = self.selection.extract_text(pane.terminal().grid()) {
                                let _ = self.clipboard.copy(&text);
                            }
                        }
                    }
                    self.copy_mode = false;
                    self.selection.clear();
                }
                "q" => {
                    // Exit copy mode
                    self.copy_mode = false;
                    self.selection.clear();
                }
                _ => {}
            },
            Key::Named(NamedKey::Escape) => {
                // Exit copy mode
                self.copy_mode = false;
                self.selection.clear();
            }
            Key::Named(NamedKey::Enter) => {
                // Copy selection and exit
                if let Some(session) = &self.session {
                    if let Some(pane) = session.active_pane() {
                        if let Some(text) = self.selection.extract_text(pane.terminal().grid()) {
                            let _ = self.clipboard.copy(&text);
                        }
                    }
                }
                self.copy_mode = false;
                self.selection.clear();
            }
            Key::Named(NamedKey::ArrowUp) => self.copy_mode_cursor.1 = row.saturating_sub(1),
            Key::Named(NamedKey::ArrowDown) => self.copy_mode_cursor.1 = (row + 1).min(max_row),
            Key::Named(NamedKey::ArrowLeft) => self.copy_mode_cursor.0 = col.saturating_sub(1),
            Key::Named(NamedKey::ArrowRight) => self.copy_mode_cursor.0 = (col + 1).min(max_col),
            _ => {}
        }

        // Update selection end point
        self.selection.update(self.copy_mode_cursor.0, self.copy_mode_cursor.1);
    }

    /// Convert winit key to our KeyCode
    fn convert_key(&self, key: &Key) -> Option<KeyCode> {
        match key {
            Key::Character(c) => {
                c.chars().next().map(KeyCode::Character)
            }
            Key::Named(named) => {
                match named {
                    NamedKey::ArrowUp => Some(KeyCode::Up),
                    NamedKey::ArrowDown => Some(KeyCode::Down),
                    NamedKey::ArrowLeft => Some(KeyCode::Left),
                    NamedKey::ArrowRight => Some(KeyCode::Right),
                    NamedKey::Home => Some(KeyCode::Home),
                    NamedKey::End => Some(KeyCode::End),
                    NamedKey::PageUp => Some(KeyCode::PageUp),
                    NamedKey::PageDown => Some(KeyCode::PageDown),
                    NamedKey::Insert => Some(KeyCode::Insert),
                    NamedKey::Delete => Some(KeyCode::Delete),
                    NamedKey::Backspace => Some(KeyCode::Backspace),
                    NamedKey::Tab => Some(KeyCode::Tab),
                    NamedKey::Enter => Some(KeyCode::Enter),
                    NamedKey::Escape => Some(KeyCode::Escape),
                    NamedKey::Space => Some(KeyCode::Space),
                    NamedKey::F1 => Some(KeyCode::F1),
                    NamedKey::F2 => Some(KeyCode::F2),
                    NamedKey::F3 => Some(KeyCode::F3),
                    NamedKey::F4 => Some(KeyCode::F4),
                    NamedKey::F5 => Some(KeyCode::F5),
                    NamedKey::F6 => Some(KeyCode::F6),
                    NamedKey::F7 => Some(KeyCode::F7),
                    NamedKey::F8 => Some(KeyCode::F8),
                    NamedKey::F9 => Some(KeyCode::F9),
                    NamedKey::F10 => Some(KeyCode::F10),
                    NamedKey::F11 => Some(KeyCode::F11),
                    NamedKey::F12 => Some(KeyCode::F12),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Convert winit modifiers to our Modifiers
    fn convert_modifiers(&self) -> Modifiers {
        Modifiers {
            ctrl: self.modifiers.control_key(),
            alt: self.modifiers.alt_key(),
            shift: self.modifiers.shift_key(),
            meta: self.modifiers.super_key(),
        }
    }

    /// Handle keyboard input
    fn handle_key_input(&mut self, event: KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }

        // Handle copy mode (vi-like navigation)
        if self.copy_mode {
            self.handle_copy_mode_key(&event);
            return;
        }

        // Handle scrollback navigation (Shift+PageUp/PageDown)
        if self.handle_scrollback_keys(&event) {
            return;
        }

        // Reset viewport to live on any other key press
        if let Some(session) = &mut self.session {
            if let Some(pane) = session.active_pane_mut() {
                if pane.terminal().is_viewing_scrollback() {
                    pane.terminal_mut().reset_viewport();
                }
            }
        }

        let mods = self.convert_modifiers();

        // Check for prefix key (configurable, default Ctrl+B)
        if let Some((ctrl, alt, shift, key)) = self.config.keybinds.parse_prefix() {
            if mods.ctrl == ctrl && mods.alt == alt && mods.shift == shift {
                if let Key::Character(c) = &event.logical_key {
                    if c.to_lowercase().chars().next() == Some(key) {
                        self.prefix_active = true;
                        self.prefix_time = Some(Instant::now());
                        return;
                    }
                }
            }
        }

        // Handle prefix mode
        if self.prefix_active {
            // Timeout after 2 seconds
            if let Some(time) = self.prefix_time {
                if time.elapsed() > Duration::from_secs(2) {
                    self.prefix_active = false;
                    self.prefix_time = None;
                }
            }

            if self.handle_prefix_key(&event.logical_key) {
                return;
            }
        }

        // Handle copy/paste
        if mods.ctrl && !mods.alt {
            if let Key::Character(c) = &event.logical_key {
                match c.as_str() {
                    "c" if mods.shift => {
                        // Ctrl+Shift+C = Copy
                        if let Some(text) = self.selection.extract_text(
                            self.session.as_ref()
                                .and_then(|s| s.active_pane())
                                .map(|p| p.terminal().grid())
                                .unwrap_or_else(|| panic!("No active pane"))
                        ) {
                            let _ = self.clipboard.copy(&text);
                        }
                        return;
                    }
                    "v" if mods.shift => {
                        // Ctrl+Shift+V = Paste
                        if let Ok(text) = self.clipboard.paste() {
                            self.write_to_pty(text.as_bytes());
                        }
                        return;
                    }
                    _ => {}
                }
            }
        }

        // Regular key input - send to PTY
        // Convert key before borrowing session to avoid borrow conflicts
        let key_code = self.convert_key(&event.logical_key);

        let Some(session) = &mut self.session else { return };
        let Some(pane) = session.active_pane_mut() else { return };

        if let Some(key_code) = key_code {
            if let Some(bytes) = pane.keyboard().key_to_bytes(key_code, &mods) {
                let _ = pane.write(&bytes);
            }
        }
    }

    /// Write bytes to the active PTY
    fn write_to_pty(&mut self, data: &[u8]) {
        let Some(session) = &mut self.session else { return };
        let Some(pane) = session.active_pane_mut() else { return };
        let _ = pane.write(data);
    }

    /// Handle mouse input
    fn handle_mouse_input(&mut self, button: MouseButton, state: ElementState) {
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                // Check for hyperlink click (Ctrl+click)
                if self.handle_hyperlink_click(self.mouse_position) {
                    return;
                }

                self.mouse_pressed = true;
                let (col, row) = self.pixel_to_cell(self.mouse_position);
                
                if self.modifiers.shift_key() {
                    // Extend existing selection
                    self.selection.update(col, row);
                } else {
                    // Start new selection
                    self.selection.start_normal(col, row);
                }

                // Reset viewport to live on click
                if let Some(session) = &mut self.session {
                    if let Some(pane) = session.active_pane_mut() {
                        pane.terminal_mut().reset_viewport();
                    }
                }
            }
            (MouseButton::Left, ElementState::Released) => {
                self.mouse_pressed = false;
            }
            (MouseButton::Middle, ElementState::Pressed) => {
                // Middle click = paste (with bracketed paste if enabled)
                if let Ok(text) = self.clipboard.paste() {
                    if let Some(session) = &mut self.session {
                        if let Some(pane) = session.active_pane_mut() {
                            // Get bracketed paste sequences first (copies values)
                            let bracketed = pane.keyboard().bracketed_paste;
                            
                            if bracketed {
                                let _ = pane.write(b"\x1b[200~");
                            }
                            let _ = pane.write(text.as_bytes());
                            if bracketed {
                                let _ = pane.write(b"\x1b[201~");
                            }
                        }
                    }
                }
            }
            (MouseButton::Right, ElementState::Pressed) => {
                // Right click = extend selection or context menu
                let (col, row) = self.pixel_to_cell(self.mouse_position);
                self.selection.update(col, row);
            }
            _ => {}
        }
    }

    /// Handle mouse movement
    fn handle_mouse_move(&mut self, position: (f64, f64)) {
        self.mouse_position = position;

        if self.mouse_pressed {
            let (col, row) = self.pixel_to_cell(position);
            self.selection.update(col, row);
        }
    }

    /// Handle mouse scroll
    fn handle_mouse_scroll(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y as i32,
            MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as i32,
        };

        let Some(session) = &mut self.session else { return };
        let Some(pane) = session.active_pane_mut() else { return };

        // Check if on alternate screen - if so, send to application
        if pane.terminal().is_alternate_screen() {
            // Send scroll events to terminal application (vim, less, etc.)
            if lines > 0 {
                for _ in 0..lines.abs() {
                    let _ = pane.write(b"\x1b[A");
                }
            } else if lines < 0 {
                for _ in 0..lines.abs() {
                    let _ = pane.write(b"\x1b[B");
                }
            }
        } else {
            // Primary screen - scroll the viewport through scrollback
            let terminal = pane.terminal_mut();
            if lines > 0 {
                // Scroll up (into history)
                terminal.scroll_viewport_up(lines.abs() as usize);
            } else if lines < 0 {
                // Scroll down (toward live)
                terminal.scroll_viewport_down(lines.abs() as usize);
            }
        }
    }

    /// Handle Shift+PageUp/PageDown for scrollback
    fn handle_scrollback_keys(&mut self, event: &KeyEvent) -> bool {
        let mods = self.convert_modifiers();
        
        if !mods.shift {
            return false;
        }

        let Some(session) = &mut self.session else { return false };
        let Some(pane) = session.active_pane_mut() else { return false };

        // Don't handle on alternate screen
        if pane.terminal().is_alternate_screen() {
            return false;
        }

        let terminal = pane.terminal_mut();
        let page_size = terminal.grid().lines() as usize;

        match &event.logical_key {
            Key::Named(NamedKey::PageUp) => {
                terminal.scroll_viewport_up(page_size);
                true
            }
            Key::Named(NamedKey::PageDown) => {
                terminal.scroll_viewport_down(page_size);
                true
            }
            Key::Named(NamedKey::Home) if mods.ctrl => {
                terminal.scroll_viewport_to_top();
                true
            }
            Key::Named(NamedKey::End) if mods.ctrl => {
                terminal.scroll_viewport_to_bottom();
                true
            }
            _ => false,
        }
    }

    /// Handle hyperlink click (Ctrl+click)
    fn handle_hyperlink_click(&mut self, position: (f64, f64)) -> bool {
        if !self.modifiers.control_key() {
            return false;
        }

        let (col, row) = self.pixel_to_cell(position);

        let Some(session) = &self.session else { return false };
        let Some(pane) = session.active_pane() else { return false };

        if let Some(hyperlink) = pane.terminal().get_cell_hyperlink(col, row) {
            // Open URL
            let url = hyperlink.url.clone();
            log::info!("Opening hyperlink: {}", url);
            
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("open")
                    .arg(&url)
                    .spawn();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("xdg-open")
                    .arg(&url)
                    .spawn();
            }
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("cmd")
                    .args(["/c", "start", &url])
                    .spawn();
            }
            
            return true;
        }

        false
    }

    /// Convert pixel position to cell coordinates
    fn pixel_to_cell(&self, position: (f64, f64)) -> (u16, u16) {
        let Some(text_renderer) = &self.text_renderer else {
            return (0, 0);
        };

        let cell_width = text_renderer.cell_width();
        let cell_height = text_renderer.cell_height();
        let padding = self.config.window.padding as f64;

        let col = ((position.0 - padding) / cell_width as f64).max(0.0) as u16;
        let row = ((position.1 - padding) / cell_height as f64).max(0.0) as u16;

        (col, row)
    }

    /// Process PTY output
    fn process_pty(&mut self) {
        let Some(session) = &mut self.session else { return };
        let _ = session.process_all();

        // Check for bell events and sync modes
        if let Some(pane) = session.active_pane_mut() {
            // Handle bell
            if let Some(_bell_event) = pane.terminal_mut().take_pending_bell() {
                self.bell.ring();
            }

            // Sync keyboard handler with terminal modes (copy modes to avoid borrow conflict)
            let modes = *pane.terminal().modes();
            pane.keyboard_mut().sync_modes(&modes);
        }

        // Update bell state
        self.bell.update();
    }

    /// Render frame
    fn render(&mut self) {
        let Some(renderer) = &mut self.renderer else { return };
        let Some(text_renderer) = &mut self.text_renderer else { return };
        let Some(session) = &self.session else { return };
        let Some(pane) = session.active_pane() else { return };

        // Get visual bell intensity for flash effect
        let bell_intensity = self.bell.visual_intensity();

        // Check if viewing scrollback
        let viewing_scrollback = pane.terminal().is_viewing_scrollback();
        let viewport_offset = pane.terminal().viewport_offset();

        // Generate vertices from terminal grid
        let (mut vertices, indices) = text_renderer.render_grid_with_viewport(
            pane.terminal().grid(),
            pane.terminal().cursor(),
            &self.selection,
            &self.config.colors,
            viewport_offset,
            viewing_scrollback,
        );

        // Apply visual bell flash effect
        if bell_intensity > 1.0 {
            for vertex in &mut vertices {
                // Brighten colors for flash
                vertex.bg_color[0] = (vertex.bg_color[0] * bell_intensity).min(1.0);
                vertex.bg_color[1] = (vertex.bg_color[1] * bell_intensity).min(1.0);
                vertex.bg_color[2] = (vertex.bg_color[2] * bell_intensity).min(1.0);
            }
        }

        // Update atlas if needed
        if text_renderer.atlas_dirty() {
            let (data, width, height) = text_renderer.atlas_data();
            renderer.update_atlas(data, width, height);
            text_renderer.mark_atlas_clean();
        }

        // Render
        let _ = renderer.render(&vertices, &indices);

        self.frame_count += 1;
        self.last_frame = Instant::now();
    }

    /// Resize terminal
    fn resize(&mut self, new_size: (u32, u32)) {
        let Some(renderer) = &mut self.renderer else { return };
        renderer.resize(new_size);

        // Calculate new terminal size
        let Some(text_renderer) = &self.text_renderer else { return };
        let cell_width = text_renderer.cell_width();
        let cell_height = text_renderer.cell_height();
        let padding = self.config.window.padding;

        let cols = ((new_size.0 - padding * 2) / cell_width as u32) as u16;
        let rows = ((new_size.1 - padding * 2) / cell_height as u32) as u16;

        if cols > 0 && rows > 0 {
            if let Some(session) = &mut self.session {
                let _ = session.resize(cols, rows);
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Create window with configured attributes
        let mut window_attrs = Window::default_attributes()
            .with_title("Basilisk")
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.window.width,
                self.config.window.height,
            ));

        // Apply window decorations setting
        use crate::config::Decorations;
        match self.config.window.decorations.to_lowercase().as_str() {
            "none" => {
                window_attrs = window_attrs.with_decorations(false);
            }
            "transparent" => {
                window_attrs = window_attrs
                    .with_decorations(true)
                    .with_transparent(true);
            }
            _ => {
                // "full" or default - standard decorations
                window_attrs = window_attrs.with_decorations(true);
            }
        }

        let opacity = self.config.window.opacity;

        let window = match event_loop.create_window(window_attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Failed to create window: {}", e);
                event_loop.exit();
                return;
            }
        };

        // Apply window opacity (platform-specific)
        if opacity < 1.0 {
            set_window_opacity(&window, opacity);
        }

        // Create renderer
        let renderer = pollster::block_on(Renderer::new(window.clone()));
        match renderer {
            Ok(r) => self.renderer = Some(r),
            Err(e) => {
                log::error!("Failed to create renderer: {}", e);
                event_loop.exit();
                return;
            }
        }

        // Create text renderer
        match TextRenderer::new(&self.config.font) {
            Ok(r) => self.text_renderer = Some(r),
            Err(e) => {
                log::error!("Failed to create text renderer: {}", e);
                event_loop.exit();
                return;
            }
        }

        // Initialize session
        if let Err(e) = self.init_session() {
            log::error!("Failed to initialize session: {}", e);
            event_loop.exit();
            return;
        }

        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                self.resize((size.width, size.height));
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_key_input(event);
            }

            WindowEvent::MouseInput { button, state, .. } => {
                self.handle_mouse_input(button, state);
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.handle_mouse_move((position.x, position.y));
            }

            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_scroll(delta);
            }

            WindowEvent::RedrawRequested => {
                self.process_pty();
                self.render();

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }

            _ => {}
        }

        if self.should_exit {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Prefix mode actions
#[derive(Debug, Clone, Copy)]
enum PrefixAction {
    NewWindow,
    SplitHorizontal,
    SplitVertical,
    NextWindow,
    PrevWindow,
    SelectWindow(usize),
    ClosePane,
    Detach,
    ZoomPane,
    CopyMode,
    FocusPaneUp,
    FocusPaneDown,
    FocusPaneLeft,
    FocusPaneRight,
}

/// Set window opacity (platform-specific)
/// 
/// Note: Full implementation requires platform-specific crates:
/// - macOS: objc/cocoa crate for NSWindow setAlphaValue
/// - Linux/X11: x11 crate for _NET_WM_WINDOW_OPACITY
/// - Windows: windows crate for SetLayeredWindowAttributes
/// 
/// For now, this logs the requested opacity. To fully implement:
/// 1. Add objc2 crate for macOS
/// 2. Add x11rb crate for Linux
/// 3. Add windows crate for Windows
fn set_window_opacity(window: &Window, opacity: f32) {
    #[cfg(target_os = "macos")]
    {
        // macOS: Would use NSWindow.setAlphaValue via objc runtime
        // Requires: objc2 = "0.5" and objc2-app-kit = "0.2"
        log::info!("macOS: Window opacity {} requested (add objc2 crate to enable)", opacity);
    }
    
    #[cfg(target_os = "linux")]
    {
        // X11: Set _NET_WM_WINDOW_OPACITY atom (CARDINAL, 32-bit opacity value)
        // Wayland: Usually not supported (compositor-dependent)
        // Requires: x11rb crate for X11 support
        log::info!("Linux: Window opacity {} requested (add x11rb crate to enable)", opacity);
    }
    
    #[cfg(target_os = "windows")]
    {
        // Windows: SetLayeredWindowAttributes with LWA_ALPHA
        // Requires: windows crate
        log::info!("Windows: Window opacity {} requested (add windows crate to enable)", opacity);
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        log::info!("Window opacity {} not supported on this platform", opacity);
    }
    
    // Suppress unused variable warning
    let _ = window;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_new() {
        let config = Config::default();
        let app = App::new(config);
        assert!(!app.prefix_active);
        assert!(!app.should_exit);
    }

    #[test]
    fn convert_modifiers() {
        let config = Config::default();
        let app = App::new(config);
        let mods = app.convert_modifiers();
        assert!(!mods.ctrl);
        assert!(!mods.alt);
    }
}
