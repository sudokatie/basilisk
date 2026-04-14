//! Input handling
//!
//! Keyboard and mouse event processing with configurable bindings.

pub mod keyboard;
pub mod bindings;
pub mod mouse;

pub use keyboard::KeyboardHandler;
pub use mouse::{MouseHandler, MouseButton, MouseEvent};
pub use crate::render::window::{KeyCode, Modifiers};
