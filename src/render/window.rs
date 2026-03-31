//! Window event loop using winit

use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId, WindowAttributes},
};
use std::sync::Arc;

/// Window event that can be handled by the application
#[derive(Debug, Clone)]
pub enum Event {
    /// Character input
    Char(char),
    /// Key press with modifiers
    Key {
        key: KeyCode,
        modifiers: Modifiers,
        pressed: bool,
    },
    /// Window resized
    Resize { width: u32, height: u32 },
    /// Window close requested
    CloseRequested,
    /// Redraw needed
    Redraw,
}

/// Key codes for special keys
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // Navigation
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    // Editing
    Backspace,
    Delete,
    Tab,
    Enter,
    Escape,
    Insert,
    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    // Other
    Space,
    Character(char),
}

/// Modifier key state
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Configuration for the window
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Basilisk".into(),
            width: 800,
            height: 600,
        }
    }
}

/// Event handler callback type
pub type EventHandler = Box<dyn FnMut(Event) -> bool>;

/// Window application state
struct App {
    window: Option<Arc<Window>>,
    config: WindowConfig,
    handler: EventHandler,
    modifiers: Modifiers,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = WindowAttributes::default()
                .with_title(&self.config.title)
                .with_inner_size(winit::dpi::LogicalSize::new(
                    self.config.width,
                    self.config.height,
                ));

            match event_loop.create_window(attrs) {
                Ok(window) => {
                    self.window = Some(Arc::new(window));
                }
                Err(e) => {
                    eprintln!("Failed to create window: {}", e);
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                (self.handler)(Event::CloseRequested);
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                (self.handler)(Event::Redraw);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }

            WindowEvent::Resized(size) => {
                (self.handler)(Event::Resize {
                    width: size.width,
                    height: size.height,
                });
            }

            WindowEvent::ModifiersChanged(mods) => {
                let state = mods.state();
                self.modifiers = Modifiers {
                    shift: state.shift_key(),
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    meta: state.super_key(),
                };
            }

            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key,
                    state,
                    ..
                },
                ..
            } => {
                let pressed = state == ElementState::Pressed;

                match logical_key.as_ref() {
                    // Handle text input
                    Key::Character(s) if pressed => {
                        for c in s.chars() {
                            let should_continue = (self.handler)(Event::Char(c));
                            if !should_continue {
                                event_loop.exit();
                                return;
                            }
                        }
                    }

                    // Handle named keys
                    Key::Named(named) => {
                        if let Some(key) = named_to_keycode(&named) {
                            let should_continue = (self.handler)(Event::Key {
                                key,
                                modifiers: self.modifiers,
                                pressed,
                            });
                            if !should_continue {
                                event_loop.exit();
                                return;
                            }
                        }
                    }

                    _ => {}
                }
            }

            _ => {}
        }
    }
}

/// Convert winit named key to our KeyCode
fn named_to_keycode(named: &NamedKey) -> Option<KeyCode> {
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

/// Run the window event loop
pub fn run_event_loop(config: WindowConfig, handler: EventHandler) -> crate::Result<()> {
    let event_loop = EventLoop::new()
        .map_err(|e| crate::Error::Render(e.to_string()))?;

    let mut app = App {
        window: None,
        config,
        handler,
        modifiers: Modifiers::default(),
    };

    event_loop.run_app(&mut app)
        .map_err(|e| crate::Error::Render(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_config_default() {
        let config = WindowConfig::default();
        assert_eq!(config.title, "Basilisk");
        assert_eq!(config.width, 800);
        assert_eq!(config.height, 600);
    }

    #[test]
    fn modifiers_default() {
        let mods = Modifiers::default();
        assert!(!mods.shift);
        assert!(!mods.ctrl);
        assert!(!mods.alt);
        assert!(!mods.meta);
    }

    #[test]
    fn keycode_variants() {
        let key = KeyCode::Up;
        assert_eq!(key, KeyCode::Up);

        let char_key = KeyCode::Character('a');
        assert_eq!(char_key, KeyCode::Character('a'));
    }

    #[test]
    fn event_variants() {
        let char_event = Event::Char('a');
        match char_event {
            Event::Char(c) => assert_eq!(c, 'a'),
            _ => panic!("Expected Char event"),
        }

        let resize = Event::Resize { width: 100, height: 200 };
        match resize {
            Event::Resize { width, height } => {
                assert_eq!(width, 100);
                assert_eq!(height, 200);
            }
            _ => panic!("Expected Resize event"),
        }
    }
}
