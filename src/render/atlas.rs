//! Glyph atlas (stub)

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub c: char,
    pub flags: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub uv: [f32; 4],
    pub size: [f32; 2],
    pub bearing: [f32; 2],
    pub advance: f32,
}

pub struct Atlas {
    width: u32,
    height: u32,
    glyphs: HashMap<GlyphKey, GlyphInfo>,
}

impl Atlas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            glyphs: HashMap::new(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}
