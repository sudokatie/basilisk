//! Input handling
//!
//! Keyboard and mouse event processing with configurable bindings.

pub mod keyboard;
pub mod bindings;

pub use keyboard::KeyboardHandler;
pub use crate::render::window::{KeyCode, Modifiers};
