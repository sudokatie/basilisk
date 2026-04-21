//! Terminal cell representation with grapheme cluster support

use bitflags::bitflags;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// RGB color
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Color palette for ANSI colors (configurable)
#[derive(Clone, Debug)]
pub struct ColorPalette {
    /// 16 ANSI colors (0-7 normal, 8-15 bright)
    colors: [Color; 16],
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self {
            colors: [
                // Normal colors (0-7)
                Color::rgb(0x28, 0x2a, 0x2e),   // 0: Black
                Color::rgb(0xa5, 0x42, 0x42),   // 1: Red
                Color::rgb(0x8c, 0x94, 0x40),   // 2: Green
                Color::rgb(0xde, 0x93, 0x5f),   // 3: Yellow
                Color::rgb(0x5f, 0x81, 0x9d),   // 4: Blue
                Color::rgb(0x85, 0x67, 0x8f),   // 5: Magenta
                Color::rgb(0x5e, 0x8d, 0x87),   // 6: Cyan
                Color::rgb(0x70, 0x78, 0x80),   // 7: White
                // Bright colors (8-15)
                Color::rgb(0x37, 0x3b, 0x41),   // 8: Bright Black
                Color::rgb(0xcc, 0x66, 0x66),   // 9: Bright Red
                Color::rgb(0xb5, 0xbd, 0x68),   // 10: Bright Green
                Color::rgb(0xf0, 0xc6, 0x74),   // 11: Bright Yellow
                Color::rgb(0x81, 0xa2, 0xbe),   // 12: Bright Blue
                Color::rgb(0xb2, 0x94, 0xbb),   // 13: Bright Magenta
                Color::rgb(0x8a, 0xbe, 0xb7),   // 14: Bright Cyan
                Color::rgb(0xc5, 0xc8, 0xc6),   // 15: Bright White
            ],
        }
    }
}

impl ColorPalette {
    /// Create palette from config colors
    pub fn from_config(
        black: Color, red: Color, green: Color, yellow: Color,
        blue: Color, magenta: Color, cyan: Color, white: Color,
        bright_black: Color, bright_red: Color, bright_green: Color, bright_yellow: Color,
        bright_blue: Color, bright_magenta: Color, bright_cyan: Color, bright_white: Color,
    ) -> Self {
        Self {
            colors: [
                black, red, green, yellow, blue, magenta, cyan, white,
                bright_black, bright_red, bright_green, bright_yellow,
                bright_blue, bright_magenta, bright_cyan, bright_white,
            ],
        }
    }

    /// Get color by ANSI index (0-15)
    pub fn get(&self, index: u8) -> Color {
        if index < 16 {
            self.colors[index as usize]
        } else {
            Color::rgb(255, 255, 255)
        }
    }

    /// Set color by ANSI index (0-15)
    pub fn set(&mut self, index: u8, color: Color) {
        if index < 16 {
            self.colors[index as usize] = color;
        }
    }
}

bitflags! {
    /// Cell attribute flags
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct CellFlags: u16 {
        const BOLD = 0b0000_0001;
        const DIM = 0b0000_0010;
        const ITALIC = 0b0000_0100;
        const UNDERLINE = 0b0000_1000;
        const BLINK = 0b0001_0000;
        const INVERSE = 0b0010_0000;
        const HIDDEN = 0b0100_0000;
        const STRIKETHROUGH = 0b1000_0000;
        const WIDE = 0b0001_0000_0000;
        const WIDE_SPACER = 0b0010_0000_0000;
    }
}

/// Grapheme cluster storage for multi-codepoint characters
#[derive(Clone, Debug, Default)]
pub struct GraphemeStorage {
    /// Map from grapheme key to grapheme string
    graphemes: HashMap<u32, String>,
    /// Next available key
    next_key: u32,
}

impl GraphemeStorage {
    pub fn new() -> Self {
        Self {
            graphemes: HashMap::new(),
            next_key: 1, // 0 means no grapheme (single char)
        }
    }

    /// Store a grapheme cluster and return its key
    pub fn store(&mut self, grapheme: &str) -> u32 {
        // Check if already stored
        for (&key, stored) in &self.graphemes {
            if stored == grapheme {
                return key;
            }
        }

        // Store new grapheme
        let key = self.next_key;
        self.next_key += 1;
        self.graphemes.insert(key, grapheme.to_string());
        key
    }

    /// Get grapheme by key
    pub fn get(&self, key: u32) -> Option<&str> {
        self.graphemes.get(&key).map(|s| s.as_str())
    }

    /// Clear storage (for memory management)
    pub fn clear(&mut self) {
        self.graphemes.clear();
        self.next_key = 1;
    }

    /// Number of stored graphemes
    pub fn len(&self) -> usize {
        self.graphemes.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.graphemes.is_empty()
    }
}

/// Thread-safe grapheme storage wrapper
pub type SharedGraphemeStorage = Arc<RwLock<GraphemeStorage>>;

/// Create a new shared grapheme storage
pub fn new_grapheme_storage() -> SharedGraphemeStorage {
    Arc::new(RwLock::new(GraphemeStorage::new()))
}

/// Single terminal cell
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Primary character (first codepoint of grapheme)
    pub c: char,
    /// Grapheme key for multi-codepoint clusters (0 = single char only)
    pub grapheme_key: u32,
    /// Foreground color
    pub fg: Color,
    /// Background color
    pub bg: Color,
    /// Cell flags (bold, italic, etc.)
    pub flags: CellFlags,
    /// Hyperlink ID (0 = no link, >0 = link reference)
    pub hyperlink_id: u32,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parse hex color string (with or without #)
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Self::rgb(r, g, b))
    }

    /// Convert 8-color ANSI code to RGB (default palette)
    pub fn from_ansi(code: u8) -> Self {
        Self::from_ansi_with_palette(code, &ColorPalette::default())
    }

    /// Convert 8-color ANSI code to RGB using palette
    pub fn from_ansi_with_palette(code: u8, palette: &ColorPalette) -> Self {
        if code < 8 {
            palette.get(code)
        } else {
            Self::rgb(255, 255, 255)
        }
    }

    /// Convert 256-color code to RGB (default palette)
    pub fn from_256(code: u8) -> Self {
        Self::from_256_with_palette(code, &ColorPalette::default())
    }

    /// Convert 256-color code to RGB using palette for first 16 colors
    pub fn from_256_with_palette(code: u8, palette: &ColorPalette) -> Self {
        match code {
            0..=15 => palette.get(code),
            16..=231 => {
                // 6x6x6 color cube
                let idx = code - 16;
                let r = (idx / 36) % 6;
                let g = (idx / 6) % 6;
                let b = idx % 6;
                Self::rgb(
                    if r == 0 { 0 } else { 55 + r * 40 },
                    if g == 0 { 0 } else { 55 + g * 40 },
                    if b == 0 { 0 } else { 55 + b * 40 },
                )
            }
            232..=255 => {
                // Grayscale
                let gray = 8 + (code - 232) * 10;
                Self::rgb(gray, gray, gray)
            }
        }
    }

    /// Convert to float array for GPU
    pub fn to_array(&self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            1.0,
        ]
    }
}

impl Cell {
    /// Create a new cell with a single character
    pub fn new(c: char) -> Self {
        Self {
            c,
            grapheme_key: 0,
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
            hyperlink_id: 0,
        }
    }

    /// Create a cell with a grapheme cluster
    pub fn with_grapheme(c: char, grapheme_key: u32) -> Self {
        Self {
            c,
            grapheme_key,
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
            hyperlink_id: 0,
        }
    }

    /// Create an empty cell
    pub fn empty() -> Self {
        Self {
            c: ' ',
            grapheme_key: 0,
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
            hyperlink_id: 0,
        }
    }

    /// Check if the cell is empty (space with no flags)
    pub fn is_empty(&self) -> bool {
        self.c == ' ' && self.grapheme_key == 0 && self.flags.is_empty() && self.hyperlink_id == 0
    }

    /// Reset the cell to empty
    pub fn reset(&mut self) {
        self.c = ' ';
        self.grapheme_key = 0;
        self.fg = Color::rgb(255, 255, 255);
        self.bg = Color::rgb(0, 0, 0);
        self.flags = CellFlags::empty();
        self.hyperlink_id = 0;
    }

    /// Check if cell has a hyperlink
    pub fn has_hyperlink(&self) -> bool {
        self.hyperlink_id != 0
    }

    /// Check if cell has a multi-codepoint grapheme
    pub fn has_grapheme(&self) -> bool {
        self.grapheme_key != 0
    }

    /// Get the display character (first char of grapheme or single char)
    pub fn display_char(&self) -> char {
        self.c
    }

    /// Get the character (compatibility method)
    pub fn c(&self) -> char {
        self.c
    }

    /// Set the cell content from a character
    pub fn set_char(&mut self, c: char) {
        self.c = c;
        self.grapheme_key = 0;
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_new() {
        let cell = Cell::new('A');
        assert_eq!(cell.c, 'A');
        assert_eq!(cell.grapheme_key, 0);
        assert!(!cell.is_empty());
    }

    #[test]
    fn cell_with_grapheme() {
        let cell = Cell::with_grapheme('👨', 42);
        assert_eq!(cell.c, '👨');
        assert_eq!(cell.grapheme_key, 42);
        assert!(cell.has_grapheme());
    }

    #[test]
    fn cell_empty() {
        let cell = Cell::empty();
        assert_eq!(cell.c, ' ');
        assert_eq!(cell.grapheme_key, 0);
        assert!(cell.is_empty());
    }

    #[test]
    fn cell_reset() {
        let mut cell = Cell::with_grapheme('X', 5);
        cell.fg = Color::rgb(255, 0, 0);
        cell.flags = CellFlags::BOLD;
        cell.reset();
        assert_eq!(cell.c, ' ');
        assert_eq!(cell.grapheme_key, 0);
        assert!(cell.flags.is_empty());
    }

    #[test]
    fn color_from_hex() {
        let red = Color::from_hex("#ff0000").unwrap();
        assert_eq!(red.r, 255);
        assert_eq!(red.g, 0);
        assert_eq!(red.b, 0);

        let green = Color::from_hex("00ff00").unwrap();
        assert_eq!(green.g, 255);
    }

    #[test]
    fn color_palette_default() {
        let palette = ColorPalette::default();
        let black = palette.get(0);
        assert_eq!(black, Color::rgb(0x28, 0x2a, 0x2e));
    }

    #[test]
    fn color_palette_custom() {
        let palette = ColorPalette::from_config(
            Color::rgb(0, 0, 0), Color::rgb(255, 0, 0), Color::rgb(0, 255, 0), Color::rgb(255, 255, 0),
            Color::rgb(0, 0, 255), Color::rgb(255, 0, 255), Color::rgb(0, 255, 255), Color::rgb(255, 255, 255),
            Color::rgb(128, 128, 128), Color::rgb(255, 128, 128), Color::rgb(128, 255, 128), Color::rgb(255, 255, 128),
            Color::rgb(128, 128, 255), Color::rgb(255, 128, 255), Color::rgb(128, 255, 255), Color::rgb(255, 255, 255),
        );
        assert_eq!(palette.get(1), Color::rgb(255, 0, 0));
        assert_eq!(palette.get(9), Color::rgb(255, 128, 128));
    }

    #[test]
    fn color_from_ansi_with_palette() {
        let mut palette = ColorPalette::default();
        palette.set(1, Color::rgb(200, 50, 50)); // Custom red

        let red = Color::from_ansi_with_palette(1, &palette);
        assert_eq!(red, Color::rgb(200, 50, 50));
    }

    #[test]
    fn color_from_256_with_palette() {
        let mut palette = ColorPalette::default();
        palette.set(9, Color::rgb(255, 100, 100)); // Custom bright red

        let bright_red = Color::from_256_with_palette(9, &palette);
        assert_eq!(bright_red, Color::rgb(255, 100, 100));

        // Colors 16+ should still use standard calculation
        let cube_color = Color::from_256_with_palette(196, &palette);
        assert_eq!(cube_color.r, 255);
    }

    #[test]
    fn color_from_256_cube() {
        // Color 196 = red in 6x6x6 cube
        let red = Color::from_256(196);
        assert_eq!(red.r, 255);
        assert_eq!(red.g, 0);
        assert_eq!(red.b, 0);
    }

    #[test]
    fn color_from_256_grayscale() {
        let gray = Color::from_256(244);
        assert_eq!(gray.r, gray.g);
        assert_eq!(gray.g, gray.b);
    }

    #[test]
    fn color_to_array() {
        let white = Color::rgb(255, 255, 255);
        let arr = white.to_array();
        assert!((arr[0] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cellflags_operations() {
        let flags = CellFlags::BOLD | CellFlags::ITALIC;
        assert!(flags.contains(CellFlags::BOLD));
        assert!(flags.contains(CellFlags::ITALIC));
        assert!(!flags.contains(CellFlags::UNDERLINE));
    }

    #[test]
    fn grapheme_storage_basic() {
        let mut storage = GraphemeStorage::new();

        // Store a family emoji
        let key1 = storage.store("👨‍👩‍👧");
        assert!(key1 > 0);
        assert_eq!(storage.get(key1), Some("👨‍👩‍👧"));

        // Store a flag emoji
        let key2 = storage.store("🇺🇸");
        assert!(key2 > 0);
        assert_ne!(key1, key2);
        assert_eq!(storage.get(key2), Some("🇺🇸"));
    }

    #[test]
    fn grapheme_storage_dedup() {
        let mut storage = GraphemeStorage::new();

        let key1 = storage.store("👨‍👩‍👧");
        let key2 = storage.store("👨‍👩‍👧");

        // Same grapheme should return same key
        assert_eq!(key1, key2);
        assert_eq!(storage.len(), 1);
    }

    #[test]
    fn grapheme_storage_clear() {
        let mut storage = GraphemeStorage::new();
        storage.store("👨‍👩‍👧");
        storage.store("🇺🇸");
        assert_eq!(storage.len(), 2);

        storage.clear();
        assert!(storage.is_empty());
    }
}
