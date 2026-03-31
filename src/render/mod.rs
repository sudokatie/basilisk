//! GPU rendering
//!
//! wgpu-based text rendering with glyph atlas caching.

pub mod glyph;
pub mod atlas;
pub mod window;

pub use window::{Event, KeyCode, Modifiers, WindowConfig, run_event_loop};
