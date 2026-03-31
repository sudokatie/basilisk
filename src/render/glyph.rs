//! Glyph rasterization (stub)

pub struct Font {
    // Will hold fontdue::Font
}

pub struct GlyphMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
}

impl Font {
    pub fn from_bytes(_data: &[u8]) -> Option<Self> {
        Some(Self {})
    }
}
