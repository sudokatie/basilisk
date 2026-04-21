//! Main window event loop
//!
//! Handles winit event loop integration and delegates to the application.

use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{NamedKey, ModifiersState};
use winit::window::{Window, WindowId, WindowAttributes};

use crate::config::Config;
use crate::render::{KeyCode, Modifiers};
use crate::Result;

/// Window state for the application
pub struct WindowState {
    /// The winit window
    pub window: Option<Arc<Window>>,
    /// Current window size
    pub size: (u32, u32),
    /// Modifier key state
    pub modifiers: ModifiersState,
    /// Mouse position in pixels
    pub mouse_position: (f64, f64),
    /// Whether a mouse button is pressed
    pub mouse_pressed: bool,
    /// Which mouse button is pressed
    pub mouse_button: Option<MouseButton>,
    /// Last click time for double/triple click detection
    pub last_click_time: Instant,
    /// Click count (1=single, 2=double, 3=triple)
    pub click_count: u8,
    /// Whether the window has focus
    pub focused: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            window: None,
            size: (800, 600),
            modifiers: ModifiersState::empty(),
            mouse_position: (0.0, 0.0),
            mouse_pressed: false,
            mouse_button: None,
            last_click_time: Instant::now(),
            click_count: 0,
            focused: true,
        }
    }
}

impl WindowState {
    /// Create new window state
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current modifiers as our Modifiers type
    pub fn get_modifiers(&self) -> Modifiers {
        Modifiers {
            ctrl: self.modifiers.control_key(),
            alt: self.modifiers.alt_key(),
            shift: self.modifiers.shift_key(),
            meta: self.modifiers.super_key(),
        }
    }

    /// Request a redraw
    pub fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Trait for applications that handle window events
pub trait WindowApp {
    /// Called when the window is created
    fn on_init(&mut self, state: &mut WindowState);

    /// Handle keyboard input, return false to exit
    fn on_key(&mut self, state: &mut WindowState, event: KeyEvent) -> bool;

    /// Handle mouse button events
    fn on_mouse_button(&mut self, state: &mut WindowState, button: MouseButton, pressed: bool);

    /// Handle mouse movement
    fn on_mouse_move(&mut self, state: &mut WindowState, x: f64, y: f64);

    /// Handle mouse scroll
    fn on_scroll(&mut self, state: &mut WindowState, delta: MouseScrollDelta);

    /// Handle window resize
    fn on_resize(&mut self, state: &mut WindowState, width: u32, height: u32);

    /// Handle focus change
    fn on_focus(&mut self, state: &mut WindowState, focused: bool);

    /// Render a frame
    fn on_render(&mut self, state: &mut WindowState);

    /// Called on each event loop iteration (for PTY polling etc)
    fn on_update(&mut self, state: &mut WindowState);

    /// Should the window close?
    fn should_exit(&self) -> bool;
}

/// Internal app wrapper for winit
struct AppWrapper<T: WindowApp> {
    app: T,
    state: WindowState,
    config: Config,
}

impl<T: WindowApp> ApplicationHandler for AppWrapper<T> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.window.is_none() {
            let title = "Basilisk";
            let width = self.config.terminal.cols as u32 * 10; // Approximate
            let height = self.config.terminal.rows as u32 * 20;

            let attrs = WindowAttributes::default()
                .with_title(title)
                .with_inner_size(winit::dpi::LogicalSize::new(width, height));

            match event_loop.create_window(attrs) {
                Ok(window) => {
                    self.state.size = (window.inner_size().width, window.inner_size().height);
                    self.state.window = Some(Arc::new(window));
                    self.app.on_init(&mut self.state);
                }
                Err(e) => {
                    eprintln!("Failed to create window: {}", e);
                    event_loop.exit();
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Poll for PTY output and other updates
        self.app.on_update(&mut self.state);

        if self.app.should_exit() {
            event_loop.exit();
            return;
        }

        // Request continuous redraws for animation
        self.state.request_redraw();

        // Use poll mode for low latency
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                self.app.on_render(&mut self.state);
            }

            WindowEvent::Resized(size) => {
                self.state.size = (size.width, size.height);
                self.app.on_resize(&mut self.state, size.width, size.height);
            }

            WindowEvent::Focused(focused) => {
                self.state.focused = focused;
                self.app.on_focus(&mut self.state, focused);
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.state.modifiers = mods.state();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if !self.app.on_key(&mut self.state, event) {
                    event_loop.exit();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.state.mouse_position = (position.x, position.y);
                self.app.on_mouse_move(&mut self.state, position.x, position.y);
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                self.state.mouse_pressed = pressed;

                if pressed {
                    self.state.mouse_button = Some(button);

                    // Detect multi-click
                    let now = Instant::now();
                    if now.duration_since(self.state.last_click_time) < Duration::from_millis(500) {
                        self.state.click_count = (self.state.click_count % 3) + 1;
                    } else {
                        self.state.click_count = 1;
                    }
                    self.state.last_click_time = now;
                } else {
                    self.state.mouse_button = None;
                }

                self.app.on_mouse_button(&mut self.state, button, pressed);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                self.app.on_scroll(&mut self.state, delta);
            }

            _ => {}
        }
    }
}

/// Run the event loop with the given application
pub fn run<T: WindowApp + 'static>(config: Config, app: T) -> Result<()> {
    let event_loop = EventLoop::new()
        .map_err(|e| crate::Error::Window(e.to_string()))?;

    let mut wrapper = AppWrapper {
        app,
        state: WindowState::new(),
        config,
    };

    event_loop.run_app(&mut wrapper)
        .map_err(|e| crate::Error::Window(e.to_string()))?;

    Ok(())
}

/// Convert winit named key to our KeyCode
pub fn convert_named_key(named: &NamedKey) -> Option<KeyCode> {
    Some(match *named {
        NamedKey::ArrowUp => KeyCode::Up,
        NamedKey::ArrowDown => KeyCode::Down,
        NamedKey::ArrowLeft => KeyCode::Left,
        NamedKey::ArrowRight => KeyCode::Right,
        NamedKey::Home => KeyCode::Home,
        NamedKey::End => KeyCode::End,
        NamedKey::PageUp => KeyCode::PageUp,
        NamedKey::PageDown => KeyCode::PageDown,
        NamedKey::Backspace => KeyCode::Backspace,
        NamedKey::Delete => KeyCode::Delete,
        NamedKey::Tab => KeyCode::Tab,
        NamedKey::Enter => KeyCode::Enter,
        NamedKey::Escape => KeyCode::Escape,
        NamedKey::Insert => KeyCode::Insert,
        NamedKey::Space => KeyCode::Space,
        NamedKey::F1 => KeyCode::F1,
        NamedKey::F2 => KeyCode::F2,
        NamedKey::F3 => KeyCode::F3,
        NamedKey::F4 => KeyCode::F4,
        NamedKey::F5 => KeyCode::F5,
        NamedKey::F6 => KeyCode::F6,
        NamedKey::F7 => KeyCode::F7,
        NamedKey::F8 => KeyCode::F8,
        NamedKey::F9 => KeyCode::F9,
        NamedKey::F10 => KeyCode::F10,
        NamedKey::F11 => KeyCode::F11,
        NamedKey::F12 => KeyCode::F12,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_state_default() {
        let state = WindowState::default();
        assert!(state.window.is_none());
        assert_eq!(state.size, (800, 600));
        assert!(!state.mouse_pressed);
        assert!(state.focused);
    }

    #[test]
    fn window_state_modifiers() {
        let state = WindowState::new();
        let mods = state.get_modifiers();
        assert!(!mods.ctrl);
        assert!(!mods.alt);
        assert!(!mods.shift);
        assert!(!mods.meta);
    }

    #[test]
    fn convert_named_key_arrows() {
        assert_eq!(convert_named_key(&NamedKey::ArrowUp), Some(KeyCode::Up));
        assert_eq!(convert_named_key(&NamedKey::ArrowDown), Some(KeyCode::Down));
        assert_eq!(convert_named_key(&NamedKey::ArrowLeft), Some(KeyCode::Left));
        assert_eq!(convert_named_key(&NamedKey::ArrowRight), Some(KeyCode::Right));
    }

    #[test]
    fn convert_named_key_function() {
        assert_eq!(convert_named_key(&NamedKey::F1), Some(KeyCode::F1));
        assert_eq!(convert_named_key(&NamedKey::F12), Some(KeyCode::F12));
    }

    #[test]
    fn convert_named_key_unknown() {
        assert_eq!(convert_named_key(&NamedKey::Copy), None);
    }
}
