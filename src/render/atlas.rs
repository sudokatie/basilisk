//! Glyph atlas for GPU text rendering
//!
//! Caches rasterized glyphs in a texture atlas to minimize GPU uploads.
//! Uses LRU eviction when the atlas becomes full.

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

/// LRU entry for tracking glyph usage
#[derive(Debug)]
struct LruEntry {
    key: GlyphKey,
    /// Row index where this glyph is stored
    row_idx: usize,
    /// Last access timestamp (frame number)
    last_access: u64,
}

/// Row in the atlas for packing glyphs
struct AtlasRow {
    y: u32,
    height: u32,
    x_cursor: u32,
    /// Keys of glyphs in this row
    glyph_keys: Vec<GlyphKey>,
}

/// Glyph texture atlas with LRU eviction
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
    /// LRU tracking
    lru: Vec<LruEntry>,
    /// Current frame number for LRU
    frame: u64,
    /// Maximum rows before eviction is triggered
    max_rows: usize,
}

impl Atlas {
    /// Create a new atlas with given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        // Estimate max rows based on typical glyph height (~20 pixels)
        let estimated_row_height = 24u32;
        let max_rows = (height / estimated_row_height) as usize;

        Self {
            data: vec![0; (width * height) as usize],
            width,
            height,
            glyphs: HashMap::new(),
            rows: Vec::new(),
            current_row: 0,
            padding: 1,
            dirty: false,
            lru: Vec::new(),
            frame: 0,
            max_rows,
        }
    }

    /// Advance frame counter (call each frame)
    pub fn advance_frame(&mut self) {
        self.frame += 1;
    }

    /// Mark a glyph as recently used
    fn touch(&mut self, key: GlyphKey, row_idx: usize) {
        if let Some(entry) = self.lru.iter_mut().find(|e| e.key == key) {
            entry.last_access = self.frame;
        } else {
            self.lru.push(LruEntry {
                key,
                row_idx,
                last_access: self.frame,
            });
        }
    }

    /// Evict least recently used row to make space
    fn evict_lru_row(&mut self) -> Option<usize> {
        if self.rows.is_empty() {
            return None;
        }

        // Find row with oldest glyphs
        let mut oldest_frame = u64::MAX;
        let mut oldest_row_idx = 0;

        for (row_idx, _row) in self.rows.iter().enumerate() {
            // Get max last_access for all glyphs in this row
            let row_oldest = self.lru.iter()
                .filter(|e| e.row_idx == row_idx)
                .map(|e| e.last_access)
                .max()
                .unwrap_or(0);

            if row_oldest < oldest_frame {
                oldest_frame = row_oldest;
                oldest_row_idx = row_idx;
            }
        }

        // Remove glyphs from evicted row
        let keys_to_remove: Vec<GlyphKey> = self.rows[oldest_row_idx].glyph_keys.clone();
        for key in &keys_to_remove {
            self.glyphs.remove(key);
        }
        self.lru.retain(|e| e.row_idx != oldest_row_idx);

        // Clear row data
        let row = &self.rows[oldest_row_idx];
        let y_start = row.y as usize;
        let y_end = (row.y + row.height) as usize;
        for y in y_start..y_end.min(self.height as usize) {
            let start = y * self.width as usize;
            let end = start + self.width as usize;
            if end <= self.data.len() {
                self.data[start..end].fill(0);
            }
        }

        // Reset row cursor
        self.rows[oldest_row_idx].x_cursor = 0;
        self.rows[oldest_row_idx].glyph_keys.clear();

        self.dirty = true;
        Some(oldest_row_idx)
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

    /// Cache a glyph, rasterizing if needed. Uses LRU eviction when atlas is full.
    pub fn cache(&mut self, key: GlyphKey, font: &Font) -> Option<GlyphInfo> {
        // Check if already cached
        if let Some(info) = self.glyphs.get(&key) {
            // Touch for LRU tracking
            if let Some(entry) = self.lru.iter_mut().find(|e| e.key == key) {
                entry.last_access = self.frame;
            }
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

        // Try to find space in atlas
        let allocation = self.allocate(
            rasterized.metrics.width as u32,
            rasterized.metrics.height as u32,
        );

        let (x, y, row_idx) = match allocation {
            Some(result) => result,
            None => {
                // Atlas full - try LRU eviction
                if let Some(evicted_row) = self.evict_lru_row() {
                    // Try allocation again after eviction
                    match self.allocate_in_row(
                        evicted_row,
                        rasterized.metrics.width as u32,
                        rasterized.metrics.height as u32,
                    ) {
                        Some((x, y)) => (x, y, evicted_row),
                        None => return None, // Still can't fit
                    }
                } else {
                    return None;
                }
            }
        };

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
        self.touch(key, row_idx);

        // Track which glyphs are in which row
        if row_idx < self.rows.len() {
            self.rows[row_idx].glyph_keys.push(key);
        }

        self.dirty = true;

        Some(info)
    }

    /// Cache a pre-rasterized glyph. Uses LRU eviction when atlas is full.
    pub fn cache_rasterized(&mut self, key: GlyphKey, rasterized: super::glyph::RasterizedGlyph) -> Option<GlyphInfo> {
        // Check if already cached
        if let Some(info) = self.glyphs.get(&key) {
            if let Some(entry) = self.lru.iter_mut().find(|e| e.key == key) {
                entry.last_access = self.frame;
            }
            return Some(*info);
        }

        if rasterized.metrics.width == 0 || rasterized.metrics.height == 0 {
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

        let allocation = self.allocate(
            rasterized.metrics.width as u32,
            rasterized.metrics.height as u32,
        );

        let (x, y, row_idx) = match allocation {
            Some(result) => result,
            None => {
                if let Some(evicted_row) = self.evict_lru_row() {
                    match self.allocate_in_row(
                        evicted_row,
                        rasterized.metrics.width as u32,
                        rasterized.metrics.height as u32,
                    ) {
                        Some((x, y)) => (x, y, evicted_row),
                        None => return None,
                    }
                } else {
                    return None;
                }
            }
        };

        self.blit(x, y, &rasterized.bitmap, rasterized.metrics.width as u32, rasterized.metrics.height as u32);

        let info = GlyphInfo {
            atlas_x: x,
            atlas_y: y,
            width: rasterized.metrics.width as u32,
            height: rasterized.metrics.height as u32,
            metrics: rasterized.metrics,
        };

        self.glyphs.insert(key, info);
        self.touch(key, row_idx);

        if row_idx < self.rows.len() {
            self.rows[row_idx].glyph_keys.push(key);
        }

        self.dirty = true;
        Some(info)
    }

    /// Allocate space for a glyph in a specific row
    fn allocate_in_row(&mut self, row_idx: usize, width: u32, height: u32) -> Option<(u32, u32)> {
        let padded_width = width + self.padding;

        if row_idx >= self.rows.len() {
            return None;
        }

        let row = &mut self.rows[row_idx];
        if row.x_cursor + padded_width <= self.width {
            let x = row.x_cursor;
            row.x_cursor += padded_width;
            return Some((x, row.y));
        }

        None
    }

    /// Allocate space for a glyph, returns (x, y, row_index)
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32, usize)> {
        let padded_width = width + self.padding;
        let padded_height = height + self.padding;

        // Try to fit in existing row
        for (i, row) in self.rows.iter_mut().enumerate() {
            if row.height >= padded_height && row.x_cursor + padded_width <= self.width {
                let x = row.x_cursor;
                row.x_cursor += padded_width;
                return Some((x, row.y, i));
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

        let row_idx = self.rows.len();
        let row = AtlasRow {
            y,
            height: padded_height,
            x_cursor: padded_width,
            glyph_keys: Vec::new(),
        };
        self.rows.push(row);

        Some((0, y, row_idx))
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
        self.lru.clear();
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
        let (x1, y1, page1) = pos1.unwrap();
        assert_eq!((x1, y1), (0, 0));
        assert_eq!(page1, 0);

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
