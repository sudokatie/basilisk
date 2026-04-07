//! Input handling
//!
//! Keyboard and mouse event processing with configurable bindings.

pub mod keyboard;
pub mod bindings;
pub mod mouse;

pub use keyboard::KeyboardHandler;
pub use bindings::{Action, Bindings, BindingState, KeyCombo, Modifiers as BindingModifiers};
pub use mouse::{MouseHandler, MouseEvent, MouseEventType, MouseButton};
pub use crate::render::window::{KeyCode, Modifiers};
