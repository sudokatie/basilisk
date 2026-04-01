//! GPU rendering
//!
//! wgpu-based text rendering with glyph atlas caching.

pub mod glyph;
pub mod atlas;
pub mod window;
pub mod renderer;
pub mod text;

pub use window::{Event, KeyCode, Modifiers, WindowConfig, run_event_loop};
pub use glyph::{Font, GlyphMetrics, RasterizedGlyph, load_system_font};
pub use atlas::{Atlas, GlyphKey, GlyphInfo};
pub use renderer::{Renderer, Vertex, color_to_array};
pub use text::TextRenderer;
