//! Emoji atlas for color glyph rendering
//!
//! Separate RGBA texture atlas for emoji and color glyphs.

use std::collections::HashMap;
use super::glyph::{Font, GlyphMetrics};

/// Key for looking up cached emoji
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmojiKey {
    /// The emoji string (may be multi-codepoint)
    pub emoji: String,
}

impl EmojiKey {
    pub fn new(s: &str) -> Self {
        Self { emoji: s.to_string() }
    }

    pub fn from_char(c: char) -> Self {
        Self { emoji: c.to_string() }
    }
}

/// Information about a cached emoji in the atlas
#[derive(Debug, Clone, Copy)]
pub struct EmojiInfo {
    /// X position in atlas texture (pixels)
    pub atlas_x: u32,
    /// Y position in atlas texture (pixels)
    pub atlas_y: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

/// Row in the atlas for packing emoji
struct AtlasRow {
    y: u32,
    height: u32,
    x_cursor: u32,
}

/// RGBA texture atlas for color emoji
pub struct EmojiAtlas {
    /// Atlas texture data (RGBA, 4 bytes per pixel)
    data: Vec<u8>,
    /// Atlas width in pixels
    width: u32,
    /// Atlas height in pixels
    height: u32,
    /// Cached emoji locations
    emoji: HashMap<EmojiKey, EmojiInfo>,
    /// Packing rows
    rows: Vec<AtlasRow>,
    /// Padding between emoji
    padding: u32,
    /// Whether atlas data changed and needs GPU upload
    dirty: bool,
    /// Emoji font for rasterization (if available)
    emoji_font: Option<Font>,
}

impl EmojiAtlas {
    /// Create a new emoji atlas with given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            data: vec![0; (width * height * 4) as usize], // RGBA
            width,
            height,
            emoji: HashMap::new(),
            rows: Vec::new(),
            padding: 2,
            dirty: false,
            emoji_font: None,
        }
    }

    /// Set the emoji font for rasterization
    pub fn set_font(&mut self, font: Font) {
        self.emoji_font = Some(font);
    }

    /// Get atlas dimensions
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get atlas texture data (RGBA)
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

    /// Look up a cached emoji
    pub fn get(&self, key: &EmojiKey) -> Option<&EmojiInfo> {
        self.emoji.get(key)
    }

    /// Check if a character is likely an emoji
    pub fn is_emoji(c: char) -> bool {
        matches!(c,
            '\u{1F300}'..='\u{1F9FF}' | // Misc Symbols, Emoticons, etc.
            '\u{2600}'..='\u{26FF}' |   // Misc Symbols
            '\u{2700}'..='\u{27BF}' |   // Dingbats
            '\u{1F600}'..='\u{1F64F}' | // Emoticons
            '\u{1F680}'..='\u{1F6FF}' | // Transport
            '\u{1F1E0}'..='\u{1F1FF}' | // Flags
            '\u{231A}'..='\u{231B}' |   // Watch, Hourglass
            '\u{23E9}'..='\u{23F3}' |   // Media controls
            '\u{23F8}'..='\u{23FA}' |   // More media
            '\u{25AA}'..='\u{25AB}' |   // Squares
            '\u{25B6}' |               // Play button
            '\u{25C0}' |               // Reverse button
            '\u{25FB}'..='\u{25FE}' |   // Squares
            '\u{2614}'..='\u{2615}' |   // Umbrella, Hot beverage
            '\u{2648}'..='\u{2653}' |   // Zodiac
            '\u{267F}' |               // Wheelchair
            '\u{2693}' |               // Anchor
            '\u{26A1}' |               // High voltage
            '\u{26AA}'..='\u{26AB}' |   // Circles
            '\u{26BD}'..='\u{26BE}' |   // Sports
            '\u{26C4}'..='\u{26C5}' |   // Weather
            '\u{26CE}' |               // Ophiuchus
            '\u{26D4}' |               // No entry
            '\u{26EA}' |               // Church
            '\u{26F2}'..='\u{26F3}' |   // Fountain, Golf
            '\u{26F5}' |               // Sailboat
            '\u{26FA}' |               // Tent
            '\u{26FD}' |               // Fuel pump
            '\u{2702}' |               // Scissors
            '\u{2705}' |               // Check mark
            '\u{2708}'..='\u{270D}' |   // Airplane to Writing hand
            '\u{270F}' |               // Pencil
            '\u{2712}' |               // Black nib
            '\u{2714}' |               // Check mark
            '\u{2716}' |               // X mark
            '\u{271D}' |               // Latin cross
            '\u{2721}' |               // Star of David
            '\u{2728}' |               // Sparkles
            '\u{2733}'..='\u{2734}' |   // Eight spoked asterisk
            '\u{2744}' |               // Snowflake
            '\u{2747}' |               // Sparkle
            '\u{274C}' |               // Cross mark
            '\u{274E}' |               // Cross mark
            '\u{2753}'..='\u{2755}' |   // Question marks
            '\u{2757}' |               // Exclamation mark
            '\u{2763}'..='\u{2764}' |   // Heart
            '\u{2795}'..='\u{2797}' |   // Math
            '\u{27A1}' |               // Right arrow
            '\u{27B0}' |               // Curly loop
            '\u{27BF}' |               // Double curly loop
            '\u{2934}'..='\u{2935}' |   // Arrows
            '\u{2B05}'..='\u{2B07}' |   // Arrows
            '\u{2B1B}'..='\u{2B1C}' |   // Squares
            '\u{2B50}' |               // Star
            '\u{2B55}' |               // Circle
            '\u{3030}' |               // Wavy dash
            '\u{303D}' |               // Part alternation mark
            '\u{3297}' |               // Circled Ideograph Congratulation
            '\u{3299}'                 // Circled Ideograph Secret
        )
    }

    /// Check if a string starts with emoji
    pub fn is_emoji_str(s: &str) -> bool {
        s.chars().next().map(Self::is_emoji).unwrap_or(false)
    }

    /// Cache an emoji. Returns info if successful.
    /// For now, creates a colored placeholder since proper emoji rasterization
    /// requires platform-specific APIs or a color font library.
    pub fn cache(&mut self, key: EmojiKey, cell_width: u32, cell_height: u32) -> Option<EmojiInfo> {
        // Check if already cached
        if let Some(info) = self.emoji.get(&key) {
            return Some(*info);
        }

        // Allocate space in atlas
        let width = cell_width * 2; // Emoji are typically double-width
        let height = cell_height;

        let (x, y) = self.allocate(width, height)?;

        // For now, draw a colored placeholder
        // In a full implementation, this would use:
        // - Core Text (macOS) with Apple Color Emoji
        // - DirectWrite (Windows) with Segoe UI Emoji
        // - FreeType + color font (Linux) with Noto Color Emoji
        self.draw_emoji_placeholder(x, y, width, height, &key.emoji);

        let info = EmojiInfo {
            atlas_x: x,
            atlas_y: y,
            width,
            height,
        };

        self.emoji.insert(key, info);
        self.dirty = true;

        Some(info)
    }

    /// Allocate space in the atlas
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let padded_width = width + self.padding;
        let padded_height = height + self.padding;

        // Try to fit in existing row
        for row in &mut self.rows {
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

    /// Draw a placeholder for an emoji (colored square with first char's hash)
    fn draw_emoji_placeholder(&mut self, x: u32, y: u32, width: u32, height: u32, emoji: &str) {
        // Generate a deterministic color from the emoji
        let hash = emoji.chars().fold(0u32, |acc, c| acc.wrapping_add(c as u32));
        let r = ((hash >> 16) & 0xFF) as u8;
        let g = ((hash >> 8) & 0xFF) as u8;
        let b = (hash & 0xFF) as u8;

        // Draw filled rectangle with slight gradient
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx;
                let py = y + dy;
                if px < self.width && py < self.height {
                    let idx = ((py * self.width + px) * 4) as usize;
                    if idx + 3 < self.data.len() {
                        // Simple gradient effect
                        let factor = 1.0 - (dy as f32 / height as f32) * 0.3;
                        self.data[idx] = (r as f32 * factor) as u8;
                        self.data[idx + 1] = (g as f32 * factor) as u8;
                        self.data[idx + 2] = (b as f32 * factor) as u8;
                        self.data[idx + 3] = 255; // Full alpha
                    }
                }
            }
        }
    }

    /// Clear the atlas
    pub fn clear(&mut self) {
        self.data.fill(0);
        self.emoji.clear();
        self.rows.clear();
        self.dirty = true;
    }

    /// Get number of cached emoji
    pub fn emoji_count(&self) -> usize {
        self.emoji.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_key_new() {
        let key = EmojiKey::new("👍");
        assert_eq!(key.emoji, "👍");
    }

    #[test]
    fn emoji_key_from_char() {
        let key = EmojiKey::from_char('😀');
        assert_eq!(key.emoji, "😀");
    }

    #[test]
    fn emoji_atlas_new() {
        let atlas = EmojiAtlas::new(512, 512);
        assert_eq!(atlas.size(), (512, 512));
        assert_eq!(atlas.data().len(), 512 * 512 * 4); // RGBA
    }

    #[test]
    fn emoji_atlas_dirty() {
        let mut atlas = EmojiAtlas::new(256, 256);
        assert!(!atlas.is_dirty());
        
        let key = EmojiKey::new("🎉");
        atlas.cache(key, 16, 20);
        assert!(atlas.is_dirty());

        atlas.mark_clean();
        assert!(!atlas.is_dirty());
    }

    #[test]
    fn emoji_atlas_cache() {
        let mut atlas = EmojiAtlas::new(256, 256);
        let key = EmojiKey::new("😀");
        
        let info = atlas.cache(key.clone(), 16, 20);
        assert!(info.is_some());
        
        let info = info.unwrap();
        assert_eq!(info.width, 32); // Double-width
        assert_eq!(info.height, 20);
    }

    #[test]
    fn emoji_atlas_get() {
        let mut atlas = EmojiAtlas::new(256, 256);
        let key = EmojiKey::new("🚀");
        
        atlas.cache(key.clone(), 16, 20);
        
        let info = atlas.get(&key);
        assert!(info.is_some());
    }

    #[test]
    fn emoji_atlas_clear() {
        let mut atlas = EmojiAtlas::new(256, 256);
        let key = EmojiKey::new("🌟");
        atlas.cache(key, 16, 20);
        
        atlas.clear();
        assert_eq!(atlas.emoji_count(), 0);
    }

    #[test]
    fn is_emoji_char() {
        assert!(EmojiAtlas::is_emoji('😀'));
        assert!(EmojiAtlas::is_emoji('🎉'));
        assert!(EmojiAtlas::is_emoji('❤'));
        assert!(!EmojiAtlas::is_emoji('A'));
        assert!(!EmojiAtlas::is_emoji('中'));
    }

    #[test]
    fn is_emoji_str() {
        assert!(EmojiAtlas::is_emoji_str("😀hello"));
        assert!(EmojiAtlas::is_emoji_str("🎉"));
        assert!(!EmojiAtlas::is_emoji_str("hello"));
        assert!(!EmojiAtlas::is_emoji_str(""));
    }

    #[test]
    fn emoji_atlas_multiple() {
        let mut atlas = EmojiAtlas::new(256, 256);
        
        atlas.cache(EmojiKey::new("😀"), 16, 20);
        atlas.cache(EmojiKey::new("🎉"), 16, 20);
        atlas.cache(EmojiKey::new("🚀"), 16, 20);
        
        assert_eq!(atlas.emoji_count(), 3);
    }

    #[test]
    fn emoji_atlas_duplicate() {
        let mut atlas = EmojiAtlas::new(256, 256);
        let key = EmojiKey::new("⭐");
        
        let info1 = atlas.cache(key.clone(), 16, 20).unwrap();
        let info2 = atlas.cache(key, 16, 20).unwrap();
        
        // Should return same info for duplicate
        assert_eq!(info1.atlas_x, info2.atlas_x);
        assert_eq!(info1.atlas_y, info2.atlas_y);
        assert_eq!(atlas.emoji_count(), 1);
    }
}
