//! Application lifecycle and main event loop

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey, ModifiersState};
use winit::window::{Window, WindowId};

use crate::config::{Config, ConfigWatcher};
use crate::input::{KeyCode, Modifiers};
use crate::mux::{Session, SessionId, SplitDirection, SessionServer, IpcMessage};
use crate::render::{Renderer, TextRenderer};
use crate::term::selection::{SelectionManager, SelectionType};
use crate::term::scrollback::ScrollbackView;
use crate::term::MouseMode;
use crate::clipboard::Clipboard;
use crate::input::bindings::{Bindings, KeyCombo, Action as BindingAction};
use std::sync::Mutex;
use crate::{Error, Result};

/// Terminal application state
pub struct App {
    config: Config,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    text_renderer: Option<TextRenderer>,
    session: Option<Session>,
    clipboard: Arc<Mutex<Clipboard>>,
    selection: SelectionManager,
    bindings: Bindings,
    
    // Prefix mode state (for tmux-like keybindings)
    prefix_active: bool,
    prefix_time: Option<Instant>,
    
    // Modifiers state
    modifiers: ModifiersState,
    
    // Mouse state
    mouse_position: (f64, f64),
    mouse_pressed: bool,
    mouse_button: Option<MouseButton>,
    last_click_time: Instant,
    click_count: u8,
    
    // Timing
    last_frame: Instant,
    frame_count: u64,
    
    // PTY communication (reserved for async reading)
    #[allow(dead_code)]
    pty_rx: Option<mpsc::Receiver<Vec<u8>>>,
    
    // Exit flag
    should_exit: bool,
    
    // Hold mode (keep window open after shell exits)
    hold_mode: bool,
    
    // Pane zoom state
    pane_zoomed: bool,
    
    // Copy mode state (vi-like navigation for selection)
    copy_mode: bool,
    copy_mode_cursor: (u16, u16), // (col, row)
    
    // Scrollback view for history navigation
    scrollback_view: ScrollbackView,
    
    // Visual bell state
    visual_bell_until: Option<Instant>,
    
    // Window has focus
    window_focused: bool,
    
    // Config file watcher for hot reload
    config_watcher: Option<ConfigWatcher>,
    config_path: Option<std::path::PathBuf>,
    
    // Search mode state
    search_mode: bool,
    search_input: String,
    
    // IPC server for attach/detach
    ipc_server: Option<SessionServer>,
    ipc_clients: Vec<std::os::unix::net::UnixStream>,
}

impl App {
    /// Create a new application with the given config
    pub fn new(config: Config) -> Self {
        let rows = config.terminal.rows;
        let bindings = Bindings::from_config(&config.keybinds.custom);
        Self {
            config,
            window: None,
            renderer: None,
            text_renderer: None,
            session: None,
            clipboard: Arc::new(Mutex::new(Clipboard::new())),
            selection: SelectionManager::new(),
            bindings,
            prefix_active: false,
            prefix_time: None,
            modifiers: ModifiersState::empty(),
            mouse_position: (0.0, 0.0),
            mouse_pressed: false,
            mouse_button: None,
            last_click_time: Instant::now(),
            click_count: 0,
            last_frame: Instant::now(),
            frame_count: 0,
            pty_rx: None,
            should_exit: false,
            hold_mode: false,
            pane_zoomed: false,
            copy_mode: false,
            copy_mode_cursor: (0, 0),
            scrollback_view: ScrollbackView::new(rows),
            visual_bell_until: None,
            window_focused: true,
            config_watcher: None,
            config_path: None,
            search_mode: false,
            search_input: String::new(),
            ipc_server: None,
            ipc_clients: Vec::new(),
        }
    }

    /// Create application with hold mode enabled
    pub fn with_hold(mut self) -> Self {
        self.hold_mode = true;
        self
    }

    /// Run the application
    pub fn run(config: Config) -> Result<()> {
        Self::run_with_options(config, false)
    }

    /// Run the application with options
    pub fn run_with_options(config: Config, hold: bool) -> Result<()> {
        let event_loop = EventLoop::new()
            .map_err(|e| Error::Window(e.to_string()))?;
        
        event_loop.set_control_flow(ControlFlow::Poll);
        
        let mut app = App::new(config);
        app.hold_mode = hold;
        
        // Set up config hot reload
        let config_path = Config::default_path();
        if config_path.exists() {
            if let Ok(watcher) = ConfigWatcher::new(config_path.clone()) {
                app.config_watcher = Some(watcher);
                app.config_path = Some(config_path);
            }
        }
        
        event_loop.run_app(&mut app)
            .map_err(|e| Error::Window(e.to_string()))?;
        
        Ok(())
    }

    /// Initialize the terminal session
    fn init_session(&mut self) -> Result<()> {
        let cols = self.config.terminal.cols;
        let rows = self.config.terminal.rows;
        let _scrollback = self.config.scrollback.lines;

        let mut session = Session::with_window(
            SessionId::new(1),
            "main".into(),
            cols,
            rows,
        );
        session.set_shell(&self.config.terminal.shell);

        // Set up clipboard callback for OSC 52
        let clipboard = Arc::clone(&self.clipboard);
        if let Some(window) = session.active_window_mut() {
            if let Some(pane) = window.active_pane_mut() {
                pane.terminal_mut().set_clipboard_callback(Box::new(move |request| {
                    if let Ok(mut clip) = clipboard.lock() {
                        match request.data {
                            Some(ref data) if !data.is_empty() => {
                                // Set clipboard
                                let _ = clip.copy(data);
                            }
                            Some(_) => {
                                // Clear clipboard (empty data)
                                let _ = clip.copy("");
                            }
                            None => {
                                // Query clipboard - response would need to be sent back to terminal
                                // This requires PTY writer to be set up, which is complex
                                // For now, queries are not supported via OSC 52
                            }
                        }
                    }
                }));
                
                // Spawn shell
                pane.spawn(&self.config.terminal.shell)?;
                
                // Set up PTY writer for terminal responses (DA, DSR, etc.)
                if let Some(master_fd) = pane.pty_fd() {
                    pane.terminal_mut().set_pty_writer(move |data: &[u8]| {
                        use std::os::unix::io::FromRawFd;
                        use std::io::Write;
                        // Safety: we're writing to a valid fd that we own
                        let mut file = unsafe { std::fs::File::from_raw_fd(master_fd) };
                        let _ = file.write_all(data);
                        // Forget the file so it doesn't close the fd
                        std::mem::forget(file);
                    });
                }
            }
        }

        self.session = Some(session);
        
        // Start IPC server for attach/detach
        if let Ok(server) = SessionServer::new("main") {
            let _ = server.set_nonblocking(true);
            self.ipc_server = Some(server);
        }
        
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
                // Save session state before exiting
                if let Err(e) = session.save_state() {
                    log::warn!("Failed to save session state: {}", e);
                }
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
                                if let Ok(mut clip) = self.clipboard.lock() {
                                    let _ = clip.copy(&text);
                                }
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
                            if let Ok(mut clip) = self.clipboard.lock() {
                                let _ = clip.copy(&text);
                            }
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

    /// Handle search mode input
    fn handle_search_mode_key(&mut self, event: &KeyEvent) {
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                // Cancel search
                self.search_mode = false;
                self.search_input.clear();
                if let Some(session) = &mut self.session {
                    if let Some(pane) = session.active_pane_mut() {
                        pane.terminal_mut().search_cancel();
                    }
                }
            }
            Key::Named(NamedKey::Enter) => {
                // Confirm search, exit search mode but keep highlights
                self.search_mode = false;
            }
            Key::Named(NamedKey::Backspace) => {
                // Delete last character
                self.search_input.pop();
                self.update_search();
            }
            Key::Character(c) => {
                // Check for n/N to navigate matches
                if !self.search_input.is_empty() {
                    match c.as_str() {
                        "n" => {
                            // Next match
                            if let Some(session) = &mut self.session {
                                if let Some(pane) = session.active_pane_mut() {
                                    pane.terminal_mut().search_next();
                                }
                            }
                            return;
                        }
                        "N" => {
                            // Previous match
                            if let Some(session) = &mut self.session {
                                if let Some(pane) = session.active_pane_mut() {
                                    pane.terminal_mut().search_prev();
                                }
                            }
                            return;
                        }
                        _ => {}
                    }
                }
                // Add character to search
                self.search_input.push_str(c);
                self.update_search();
            }
            _ => {}
        }
    }

    /// Update search in terminal
    fn update_search(&mut self) {
        if let Some(session) = &mut self.session {
            if let Some(pane) = session.active_pane_mut() {
                if self.search_input.is_empty() {
                    pane.terminal_mut().search_cancel();
                } else {
                    pane.terminal_mut().search_start(&self.search_input);
                }
            }
        }
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

        // Handle search mode
        if self.search_mode {
            self.handle_search_mode_key(&event);
            return;
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

        // Handle scrollback navigation with Shift+PageUp/Down
        if mods.shift && !mods.ctrl && !mods.alt {
            match &event.logical_key {
                Key::Named(NamedKey::PageUp) => {
                    if let Some(session) = &self.session {
                        if let Some(pane) = session.active_pane() {
                            self.scrollback_view.page_up(pane.terminal().grid());
                        }
                    }
                    return;
                }
                Key::Named(NamedKey::PageDown) => {
                    self.scrollback_view.page_down();
                    return;
                }
                Key::Named(NamedKey::Home) => {
                    if let Some(session) = &self.session {
                        if let Some(pane) = session.active_pane() {
                            self.scrollback_view.scroll_to_top(pane.terminal().grid());
                        }
                    }
                    return;
                }
                Key::Named(NamedKey::End) => {
                    self.scrollback_view.scroll_to_bottom();
                    return;
                }
                _ => {}
            }
        }

        // Handle copy/paste and search
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
                            if let Ok(mut clip) = self.clipboard.lock() {
                                let _ = clip.copy(&text);
                            }
                        }
                        return;
                    }
                    "v" if mods.shift => {
                        // Ctrl+Shift+V = Paste (with bracketed paste support)
                        let text = self.clipboard.lock()
                            .ok()
                            .and_then(|clip| clip.paste().ok());
                        if let Some(text) = text {
                            self.write_paste(&text);
                        }
                        return;
                    }
                    "f" if mods.shift => {
                        // Ctrl+Shift+F = Enter search mode
                        self.search_mode = true;
                        self.search_input.clear();
                        return;
                    }
                    _ => {}
                }
            }
        }

        // Exit scrollback mode on any key (except scroll keys)
        if self.scrollback_view.is_active() {
            self.scrollback_view.scroll_to_bottom();
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

    /// Write paste data with optional bracketed paste mode
    fn write_paste(&mut self, text: &str) {
        let bracketed = self.session.as_ref()
            .and_then(|s| s.active_pane())
            .map(|p| p.terminal().modes().bracketed_paste)
            .unwrap_or(false);
        
        if bracketed {
            self.write_to_pty(b"\x1b[200~");
            self.write_to_pty(text.as_bytes());
            self.write_to_pty(b"\x1b[201~");
        } else {
            self.write_to_pty(text.as_bytes());
        }
    }

    /// Send mouse event to PTY based on current mouse mode
    fn send_mouse_event(&mut self, button: u8, col: u16, row: u16, pressed: bool) {
        let mouse_mode = self.session.as_ref()
            .and_then(|s| s.active_pane())
            .map(|p| p.terminal().modes().mouse_tracking)
            .unwrap_or(MouseMode::None);
        
        // Clamp coordinates to fit in legacy protocol (max 223)
        let col_byte = (col.min(222) + 1 + 32) as u8;
        let row_byte = (row.min(222) + 1 + 32) as u8;
        
        match mouse_mode {
            MouseMode::None => return,
            MouseMode::X10 => {
                if !pressed { return; } // X10 only reports button press
                let bytes = [0x1b, b'[', b'M', 32 + button, col_byte, row_byte];
                self.write_to_pty(&bytes);
            }
            MouseMode::Normal | MouseMode::ButtonMotion | MouseMode::AnyMotion => {
                let btn = if pressed { button } else { 3 }; // 3 = release
                let bytes = [0x1b, b'[', b'M', 32 + btn, col_byte, row_byte];
                self.write_to_pty(&bytes);
            }
            MouseMode::Sgr => {
                let action = if pressed { 'M' } else { 'm' };
                let bytes = format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, action);
                self.write_to_pty(bytes.as_bytes());
            }
        }
    }

    /// Send focus event to PTY
    fn send_focus_event(&mut self, focused: bool) {
        let should_report = self.session.as_ref()
            .and_then(|s| s.active_pane())
            .map(|p| p.terminal().modes().focus_reporting)
            .unwrap_or(false);
        
        if should_report {
            let seq = if focused { b"\x1b[I" } else { b"\x1b[O" };
            self.write_to_pty(seq);
        }
    }

    /// Trigger visual bell
    fn trigger_visual_bell(&mut self) {
        self.visual_bell_until = Some(Instant::now() + Duration::from_millis(100));
    }

    /// Check if visual bell is active
    fn is_visual_bell_active(&self) -> bool {
        self.visual_bell_until.map(|t| Instant::now() < t).unwrap_or(false)
    }

    /// Handle mouse input
    fn handle_mouse_input(&mut self, button: MouseButton, state: ElementState) {
        let (col, row) = self.pixel_to_cell(self.mouse_position);
        
        // Check if terminal wants mouse events
        let mouse_mode = self.session.as_ref()
            .and_then(|s| s.active_pane())
            .map(|p| p.terminal().modes().mouse_tracking)
            .unwrap_or(MouseMode::None);
        
        // Map button to protocol number
        let button_num = match button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            _ => 0,
        };
        
        // If mouse tracking is enabled, send to terminal
        if mouse_mode != MouseMode::None {
            let pressed = state == ElementState::Pressed;
            self.send_mouse_event(button_num, col, row, pressed);
            return;
        }
        
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                self.mouse_pressed = true;
                self.mouse_button = Some(MouseButton::Left);
                
                // Check for hyperlink click
                if let Some(session) = &self.session {
                    if let Some(pane) = session.active_pane() {
                        let cell = pane.terminal().grid().cell(col, row);
                        if cell.has_hyperlink() {
                            if let Some(link) = pane.terminal().hyperlink(cell.hyperlink_id) {
                                // Open hyperlink (platform-specific)
                                let _ = open::that(&link.url);
                                return;
                            }
                        }
                    }
                }
                
                // Detect double/triple click
                let now = Instant::now();
                let double_click_threshold = Duration::from_millis(400);
                
                if now.duration_since(self.last_click_time) < double_click_threshold {
                    self.click_count += 1;
                } else {
                    self.click_count = 1;
                }
                self.last_click_time = now;
                
                if self.modifiers.shift_key() {
                    // Extend existing selection
                    self.selection.update(col, row);
                } else if self.click_count == 2 {
                    // Double-click = word selection
                    self.selection.start(col, row, SelectionType::Word);
                    self.expand_word_selection(col, row);
                } else if self.click_count >= 3 {
                    // Triple-click = line selection
                    self.selection.start(col, row, SelectionType::Line);
                } else {
                    // Single click = start new selection
                    self.selection.start_normal(col, row);
                }
            }
            (MouseButton::Left, ElementState::Released) => {
                self.mouse_pressed = false;
                self.mouse_button = None;
            }
            (MouseButton::Middle, ElementState::Pressed) => {
                self.mouse_button = Some(MouseButton::Middle);
                // Middle click = paste (with bracketed paste support)
                let text = self.clipboard.lock()
                    .ok()
                    .and_then(|clip| clip.paste().ok());
                if let Some(text) = text {
                    self.write_paste(&text);
                }
            }
            (MouseButton::Middle, ElementState::Released) => {
                self.mouse_button = None;
            }
            (MouseButton::Right, ElementState::Pressed) => {
                self.mouse_button = Some(MouseButton::Right);
                // Right click = extend selection or context menu
                self.selection.update(col, row);
            }
            (MouseButton::Right, ElementState::Released) => {
                self.mouse_button = None;
            }
            _ => {}
        }
    }

    /// Expand selection to include full word
    fn expand_word_selection(&mut self, col: u16, row: u16) {
        let Some(session) = &self.session else { return };
        let Some(pane) = session.active_pane() else { return };
        let grid = pane.terminal().grid();
        
        // Find word boundaries
        let mut start_col = col;
        let mut end_col = col;
        
        // Go backwards to find start of word
        while start_col > 0 {
            let c = grid.cell(start_col - 1, row).c;
            if c.is_whitespace() || c == '\0' {
                break;
            }
            start_col -= 1;
        }
        
        // Go forwards to find end of word
        let max_col = grid.cols().saturating_sub(1);
        while end_col < max_col {
            let c = grid.cell(end_col + 1, row).c;
            if c.is_whitespace() || c == '\0' {
                break;
            }
            end_col += 1;
        }
        
        // Update selection
        self.selection.start(start_col, row, SelectionType::Word);
        self.selection.update(end_col, row);
    }

    /// Handle mouse movement
    fn handle_mouse_move(&mut self, position: (f64, f64)) {
        let old_cell = self.pixel_to_cell(self.mouse_position);
        self.mouse_position = position;
        let new_cell = self.pixel_to_cell(position);
        
        // Check if terminal wants motion events
        let mouse_mode = self.session.as_ref()
            .and_then(|s| s.active_pane())
            .map(|p| p.terminal().modes().mouse_tracking)
            .unwrap_or(MouseMode::None);
        
        // Send motion events if appropriate
        match mouse_mode {
            MouseMode::AnyMotion => {
                if old_cell != new_cell {
                    // Button + 32 for motion flag
                    let btn = if self.mouse_pressed {
                        // Use actual pressed button number
                        let base = match self.mouse_button {
                            Some(MouseButton::Left) => 0,
                            Some(MouseButton::Middle) => 1,
                            Some(MouseButton::Right) => 2,
                            _ => 0,
                        };
                        base + 32 // Add motion flag
                    } else {
                        35 // No button pressed, motion only
                    };
                    self.send_mouse_event(btn, new_cell.0, new_cell.1, true);
                }
            }
            MouseMode::ButtonMotion if self.mouse_pressed => {
                if old_cell != new_cell {
                    // Motion while button pressed - use correct button number
                    let btn = match self.mouse_button {
                        Some(MouseButton::Left) => 32,   // Button 0 + motion flag
                        Some(MouseButton::Middle) => 33, // Button 1 + motion flag
                        Some(MouseButton::Right) => 34,  // Button 2 + motion flag
                        _ => 32,
                    };
                    self.send_mouse_event(btn, new_cell.0, new_cell.1, true);
                }
            }
            _ => {
                // Regular selection update
                if self.mouse_pressed {
                    self.selection.update(new_cell.0, new_cell.1);
                }
            }
        }
    }

    /// Handle mouse scroll
    fn handle_mouse_scroll(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y as i32,
            MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as i32,
        };

        if lines == 0 {
            return;
        }

        // Check if terminal wants mouse events
        let mouse_mode = self.session.as_ref()
            .and_then(|s| s.active_pane())
            .map(|p| p.terminal().modes().mouse_tracking)
            .unwrap_or(MouseMode::None);
        
        // Check if on alternate screen (vim, etc.)
        let on_alt_screen = self.session.as_ref()
            .and_then(|s| s.active_pane())
            .map(|p| p.terminal().is_alternate_screen())
            .unwrap_or(false);

        let (col, row) = self.pixel_to_cell(self.mouse_position);

        if mouse_mode != MouseMode::None {
            // Send scroll as button 64/65 (wheel up/down)
            let button = if lines > 0 { 64 } else { 65 };
            for _ in 0..lines.abs() {
                self.send_mouse_event(button, col, row, true);
            }
        } else if on_alt_screen {
            // On alternate screen, send arrow keys
            let key = if lines > 0 { b"\x1b[A" } else { b"\x1b[B" };
            for _ in 0..lines.abs() {
                self.write_to_pty(key);
            }
        } else {
            // Normal mode: scroll through scrollback
            if let Some(session) = &self.session {
                if let Some(pane) = session.active_pane() {
                    if lines > 0 {
                        self.scrollback_view.scroll_up(lines.abs() as usize, pane.terminal().grid());
                    } else {
                        self.scrollback_view.scroll_down(lines.abs() as usize);
                    }
                }
            }
        }
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
    }

    /// Render frame
    fn render(&mut self) {
        // Check visual bell state before borrowing
        let visual_bell_active = self.is_visual_bell_active();
        let copy_mode = self.copy_mode;
        let copy_mode_cursor = self.copy_mode_cursor;
        let search_mode = self.search_mode;
        let search_input = self.search_input.clone();

        // Collect window info for status bar before mutable borrows
        let window_info: Vec<(String, bool)> = self.session.as_ref()
            .map(|s| {
                let active_idx = s.active_window_index();
                s.windows().iter().enumerate()
                    .map(|(i, w)| (w.name().to_string(), i == active_idx))
                    .collect()
            })
            .unwrap_or_default();

        let Some(renderer) = &mut self.renderer else { return };
        let Some(text_renderer) = &mut self.text_renderer else { return };
        let Some(session) = &mut self.session else { return };
        let Some(pane) = session.active_pane_mut() else { return };

        // Update cursor blink
        pane.terminal_mut().update_cursor_blink();

        // Generate vertices from terminal grid
        let (mut vertices, mut indices) = text_renderer.render_grid(
            pane.terminal().grid(),
            pane.terminal().cursor(),
            &self.selection,
            &self.config.colors,
        );

        // Render status bar with window list
        if !window_info.is_empty() {
            text_renderer.render_status_bar(
                &window_info,
                if search_mode { Some(&search_input) } else { None },
                &mut vertices,
                &mut indices,
            );
        }

        // Render visual bell if active
        if visual_bell_active {
            text_renderer.render_visual_bell(&mut vertices, &mut indices);
        }

        // Render copy mode cursor distinctly
        if copy_mode {
            text_renderer.render_copy_mode_cursor(
                copy_mode_cursor.0,
                copy_mode_cursor.1,
                &mut vertices,
                &mut indices,
            );
        }

        // Update atlas if needed
        if text_renderer.atlas_dirty() {
            let (data, width, height) = text_renderer.atlas_data();
            renderer.update_atlas(data, width, height);
            text_renderer.mark_atlas_clean();
        }

        // Upload and render sixel images
        let sixel_images = pane.terminal().sixel_images();
        if !sixel_images.is_empty() {
            let cell_width = text_renderer.cell_width();
            let cell_height = text_renderer.cell_height();
            
            // Clear previous images and upload new ones
            renderer.clear_images();
            
            let mut image_draws: Vec<(usize, Vec<crate::render::renderer::Vertex>, Vec<u32>)> = Vec::new();
            
            for img in sixel_images {
                // Upload image to GPU
                let img_idx = renderer.upload_image(&img.data, img.width, img.height, img.col, img.row);
                
                // Generate quad vertices for this image
                let x = img.col as f32 * cell_width;
                let y = img.row as f32 * cell_height;
                let w = img.width as f32;
                let h = img.height as f32;
                
                // Convert to clip space
                let screen_w = renderer.size().0 as f32;
                let screen_h = renderer.size().1 as f32;
                
                let x0 = (x / screen_w) * 2.0 - 1.0;
                let y0 = 1.0 - (y / screen_h) * 2.0;
                let x1 = ((x + w) / screen_w) * 2.0 - 1.0;
                let y1 = 1.0 - ((y + h) / screen_h) * 2.0;
                
                let img_vertices = vec![
                    crate::render::renderer::Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0], bg_color: [0.0, 0.0, 0.0, 0.0] },
                    crate::render::renderer::Vertex { position: [x1, y0], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0], bg_color: [0.0, 0.0, 0.0, 0.0] },
                    crate::render::renderer::Vertex { position: [x1, y1], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0], bg_color: [0.0, 0.0, 0.0, 0.0] },
                    crate::render::renderer::Vertex { position: [x0, y1], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0], bg_color: [0.0, 0.0, 0.0, 0.0] },
                ];
                let img_indices = vec![0, 1, 2, 0, 2, 3];
                
                image_draws.push((img_idx, img_vertices, img_indices));
            }
            
            // Render with images
            let image_refs: Vec<(usize, &[crate::render::renderer::Vertex], &[u32])> = image_draws
                .iter()
                .map(|(idx, v, i)| (*idx, v.as_slice(), i.as_slice()))
                .collect();
            let _ = renderer.render_with_images(&vertices, &indices, &image_refs);
        } else {
            // Render without images
            let _ = renderer.render(&vertices, &indices);
        }

        self.frame_count += 1;
        self.last_frame = Instant::now();
    }

    /// Set window opacity (platform-specific)
    #[cfg(target_os = "macos")]
    fn set_window_opacity(&self, window: &Window, opacity: f32) {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::AppKit(appkit) = handle.as_raw() {
                // Safety: We have a valid AppKit window handle
                // On macOS, we get ns_view but need to get its window
                unsafe {
                    use objc2::runtime::AnyObject;
                    use objc2::msg_send;
                    
                    let ns_view: *mut AnyObject = appkit.ns_view.as_ptr() as *mut _;
                    if !ns_view.is_null() {
                        // Get the window from the view
                        let ns_window: *mut AnyObject = msg_send![ns_view, window];
                        if !ns_window.is_null() {
                            let _: () = msg_send![ns_window, setAlphaValue: opacity as f64];
                        }
                    }
                }
            }
        }
    }

    /// Set window opacity (platform-specific)
    #[cfg(not(target_os = "macos"))]
    fn set_window_opacity(&self, _window: &Window, opacity: f32) {
        // Window opacity is not directly supported on this platform via winit
        // On Linux/Wayland: compositor handles transparency
        // On Windows: Would need platform-specific APIs
        if opacity < 1.0 {
            log::warn!("Window opacity not supported on this platform. \
                       Use compositor settings for transparency.");
        }
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

    /// Poll IPC server for connections and messages
    fn poll_ipc(&mut self) {
        use std::io::{Read, Write};
        
        // Accept new connections
        if let Some(ref server) = self.ipc_server {
            if let Ok(stream) = server.accept() {
                let _ = stream.set_nonblocking(true);
                self.ipc_clients.push(stream);
            }
        }

        // Process messages from clients
        let mut clients_to_remove = Vec::new();
        let mut messages_to_process = Vec::new();

        for (idx, client) in self.ipc_clients.iter_mut().enumerate() {
            // Try to read message length
            let mut len_buf = [0u8; 4];
            match client.read_exact(&mut len_buf) {
                Ok(_) => {
                    let len = u32::from_le_bytes(len_buf) as usize;
                    let mut data = vec![0u8; len];
                    if client.read_exact(&mut data).is_ok() {
                        if let Some(msg) = IpcMessage::from_bytes(&data) {
                            messages_to_process.push((idx, msg));
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available
                }
                Err(_) => {
                    clients_to_remove.push(idx);
                }
            }
        }

        // Handle messages
        for (idx, msg) in messages_to_process {
            match msg {
                IpcMessage::Attach => {
                    // Send acknowledgment with terminal size
                    let (cols, rows) = self.session.as_ref()
                        .and_then(|s| s.active_pane())
                        .map(|p| (p.terminal().grid().cols(), p.terminal().grid().lines()))
                        .unwrap_or((80, 24));
                    
                    let ack = IpcMessage::AttachAck { cols, rows };
                    let data = ack.to_bytes();
                    let len = (data.len() as u32).to_le_bytes();
                    if let Some(client) = self.ipc_clients.get_mut(idx) {
                        let _ = client.write_all(&len);
                        let _ = client.write_all(&data);
                        let _ = client.flush();
                    }
                }
                IpcMessage::Input(data) => {
                    // Forward input to PTY
                    self.write_to_pty(&data);
                }
                IpcMessage::Detach => {
                    clients_to_remove.push(idx);
                }
                IpcMessage::Resize { cols, rows } => {
                    // Resize terminal
                    if let Some(session) = &mut self.session {
                        if let Some(pane) = session.active_pane_mut() {
                            pane.resize(cols, rows);
                        }
                    }
                }
                _ => {}
            }
        }

        // Remove disconnected clients (in reverse order to preserve indices)
        clients_to_remove.sort();
        clients_to_remove.reverse();
        for idx in clients_to_remove {
            self.ipc_clients.remove(idx);
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
        match self.config.window.decorations.to_lowercase().as_str() {
            "none" => {
                window_attrs = window_attrs.with_decorations(false);
            }
            "transparent" => {
                // Transparent background (works with compositor on supported platforms)
                // On macOS: enables per-pixel alpha
                // On Wayland: enables transparent windows with compositor support
                window_attrs = window_attrs
                    .with_decorations(true)
                    .with_transparent(true);
            }
            _ => {
                // "full" or default - standard decorations
                window_attrs = window_attrs.with_decorations(true);
            }
        }

        // Apply blur effect for transparent windows on supported platforms
        #[cfg(target_os = "macos")]
        if self.config.window.decorations.to_lowercase() == "transparent" {
            // macOS handles transparency through compositor
        }

        let window = match event_loop.create_window(window_attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Failed to create window: {}", e);
                event_loop.exit();
                return;
            }
        };

        // Set window opacity via platform-specific APIs
        let opacity = self.config.window.opacity;
        if opacity < 1.0 {
            self.set_window_opacity(&window, opacity);
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

            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
                self.send_focus_event(focused);
            }

            WindowEvent::RedrawRequested => {
                self.process_pty();
                
                // Check for visual bell
                if let Some(session) = &mut self.session {
                    if let Some(pane) = session.active_pane_mut() {
                        if pane.terminal_mut().take_bell_pending() {
                            self.trigger_visual_bell();
                        }
                    }
                }
                
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
        // Check for config hot reload
        if let (Some(watcher), Some(path)) = (&self.config_watcher, &self.config_path) {
            if watcher.check_modified() {
                if let Some(new_config) = Config::reload(path, &self.config) {
                    log::info!("Config reloaded");
                    // Apply color changes
                    self.config.colors = new_config.colors;
                    // Apply window opacity if changed
                    if (new_config.window.opacity - self.config.window.opacity).abs() > 0.01 {
                        self.config.window.opacity = new_config.window.opacity;
                        if let Some(window) = &self.window {
                            self.set_window_opacity(window, new_config.window.opacity);
                        }
                    }
                    // Note: Font changes would require recreating the text renderer
                }
            }
        }

        // Poll IPC server for new connections and messages
        self.poll_ipc();

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
