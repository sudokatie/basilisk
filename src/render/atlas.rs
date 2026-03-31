//! Glyph atlas for GPU text rendering
//!
//! Caches rasterized glyphs in a texture atlas to minimize GPU uploads.

use std::collections::HashMap;
use super::glyph::{Font, GlyphMetrics};

/// Key for looking up cached glyphs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub character: char,
    pub flags: u8, // bold, italic, etc.
}

impl GlyphKey {
    pub fn new(c: char, flags: u8) -> Self {
        Self { character: c, flags }
    }

    pub fn regular(c: char) -> Self {
        Self::new(c, 0)
    }

    pub fn bold(c: char) -> Self {
        Self::new(c, 1)
    }

    pub fn italic(c: char) -> Self {
        Self::new(c, 2)
    }
}

/// Information about a cached glyph in the atlas
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// X position in atlas texture (pixels)
    pub atlas_x: u32,
    /// Y position in atlas texture (pixels)
    pub atlas_y: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Glyph metrics for positioning
    pub metrics: GlyphMetrics,
}

/// Row in the atlas for packing glyphs
struct AtlasRow {
    y: u32,
    height: u32,
    x_cursor: u32,
}

/// Glyph texture atlas
pub struct Atlas {
    /// Atlas texture data (grayscale)
    data: Vec<u8>,
    /// Atlas width in pixels
    width: u32,
    /// Atlas height in pixels
    height: u32,
    /// Cached glyph locations
    glyphs: HashMap<GlyphKey, GlyphInfo>,
    /// Packing rows
    rows: Vec<AtlasRow>,
    /// Current row being packed into
    current_row: usize,
    /// Padding between glyphs
    padding: u32,
    /// Whether atlas data changed and needs GPU upload
    dirty: bool,
}

impl Atlas {
    /// Create a new atlas with given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            data: vec![0; (width * height) as usize],
            width,
            height,
            glyphs: HashMap::new(),
            rows: Vec::new(),
            current_row: 0,
            padding: 1,
            dirty: false,
        }
    }

    /// Get atlas dimensions
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get atlas texture data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Check if atlas needs GPU upload
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark atlas as uploaded
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Look up a cached glyph
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphInfo> {
        self.glyphs.get(key)
    }

    /// Cache a glyph, rasterizing if needed
    pub fn cache(&mut self, key: GlyphKey, font: &Font) -> Option<GlyphInfo> {
        // Check if already cached
        if let Some(info) = self.glyphs.get(&key) {
            return Some(*info);
        }

        // Rasterize the glyph
        let rasterized = font.rasterize(key.character);

        if rasterized.metrics.width == 0 || rasterized.metrics.height == 0 {
            // Empty glyph (space, etc.) - still cache metrics
            let info = GlyphInfo {
                atlas_x: 0,
                atlas_y: 0,
                width: 0,
                height: 0,
                metrics: rasterized.metrics,
            };
            self.glyphs.insert(key, info);
            return Some(info);
        }

        // Find space in atlas
        let (x, y) = self.allocate(
            rasterized.metrics.width as u32,
            rasterized.metrics.height as u32,
        )?;

        // Copy bitmap to atlas
        self.blit(x, y, &rasterized.bitmap, rasterized.metrics.width as u32, rasterized.metrics.height as u32);

        let info = GlyphInfo {
            atlas_x: x,
            atlas_y: y,
            width: rasterized.metrics.width as u32,
            height: rasterized.metrics.height as u32,
            metrics: rasterized.metrics,
        };

        self.glyphs.insert(key, info);
        self.dirty = true;

        Some(info)
    }

    /// Allocate space for a glyph
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let padded_width = width + self.padding;
        let padded_height = height + self.padding;

        // Try to fit in existing row
        for (i, row) in self.rows.iter_mut().enumerate() {
            if row.height >= padded_height && row.x_cursor + padded_width <= self.width {
                let x = row.x_cursor;
                row.x_cursor += padded_width;
                return Some((x, row.y));
            }
        }

        // Need a new row
        let y = if self.rows.is_empty() {
            0
        } else {
            let last = self.rows.last().unwrap();
            last.y + last.height
        };

        if y + padded_height > self.height {
            // Atlas full
            return None;
        }

        let row = AtlasRow {
            y,
            height: padded_height,
            x_cursor: padded_width,
        };
        self.rows.push(row);

        Some((0, y))
    }

    /// Copy bitmap data to atlas
    fn blit(&mut self, x: u32, y: u32, bitmap: &[u8], width: u32, height: u32) {
        for row in 0..height {
            let src_start = (row * width) as usize;
            let src_end = src_start + width as usize;
            let dst_start = ((y + row) * self.width + x) as usize;

            if dst_start + width as usize <= self.data.len() && src_end <= bitmap.len() {
                self.data[dst_start..dst_start + width as usize]
                    .copy_from_slice(&bitmap[src_start..src_end]);
            }
        }
    }

    /// Clear the atlas
    pub fn clear(&mut self) {
        self.data.fill(0);
        self.glyphs.clear();
        self.rows.clear();
        self.current_row = 0;
        self.dirty = true;
    }

    /// Get number of cached glyphs
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_key_new() {
        let key = GlyphKey::new('A', 0);
        assert_eq!(key.character, 'A');
        assert_eq!(key.flags, 0);
    }

    #[test]
    fn glyph_key_variants() {
        let regular = GlyphKey::regular('B');
        assert_eq!(regular.flags, 0);

        let bold = GlyphKey::bold('B');
        assert_eq!(bold.flags, 1);

        let italic = GlyphKey::italic('B');
        assert_eq!(italic.flags, 2);
    }

    #[test]
    fn glyph_key_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(GlyphKey::regular('A'));
        set.insert(GlyphKey::bold('A'));

        assert_eq!(set.len(), 2);
        assert!(set.contains(&GlyphKey::regular('A')));
    }

    #[test]
    fn atlas_new() {
        let atlas = Atlas::new(512, 512);
        assert_eq!(atlas.size(), (512, 512));
        assert_eq!(atlas.data().len(), 512 * 512);
    }

    #[test]
    fn atlas_dirty_flag() {
        let mut atlas = Atlas::new(256, 256);
        assert!(!atlas.is_dirty());

        atlas.dirty = true;
        assert!(atlas.is_dirty());

        atlas.mark_clean();
        assert!(!atlas.is_dirty());
    }

    #[test]
    fn atlas_clear() {
        let mut atlas = Atlas::new(256, 256);
        atlas.data[0] = 255;
        atlas.clear();

        assert_eq!(atlas.data[0], 0);
        assert_eq!(atlas.glyph_count(), 0);
    }

    #[test]
    fn atlas_allocate() {
        let mut atlas = Atlas::new(256, 256);

        // First allocation should succeed
        let pos1 = atlas.allocate(32, 32);
        assert!(pos1.is_some());
        assert_eq!(pos1.unwrap(), (0, 0));

        // Second allocation in same row
        let pos2 = atlas.allocate(32, 32);
        assert!(pos2.is_some());
        assert_eq!(pos2.unwrap().0, 33); // After first glyph + padding
    }

    #[test]
    fn atlas_allocate_new_row() {
        let mut atlas = Atlas::new(64, 256);

        // Fill first row
        atlas.allocate(30, 20);
        atlas.allocate(30, 20);

        // Should go to new row
        let pos = atlas.allocate(30, 20);
        assert!(pos.is_some());
        assert_eq!(pos.unwrap().1, 21); // New row
    }

    #[test]
    fn glyph_info_fields() {
        let info = GlyphInfo {
            atlas_x: 10,
            atlas_y: 20,
            width: 8,
            height: 12,
            metrics: GlyphMetrics {
                advance_width: 10.0,
                width: 8,
                height: 12,
                xmin: 1,
                ymin: -2,
            },
        };
        assert_eq!(info.atlas_x, 10);
        assert_eq!(info.metrics.advance_width, 10.0);
    }
}
