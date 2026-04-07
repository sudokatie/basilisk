//! Terminal cell representation with grapheme cluster support

use bitflags::bitflags;
use std::sync::Arc;

/// RGB color
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
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

/// Content of a cell - either a single char or a grapheme cluster
#[derive(Clone, Debug, PartialEq)]
pub enum CellContent {
    /// Single ASCII or BMP character (common case, no allocation)
    Char(char),
    /// Grapheme cluster (multiple codepoints, e.g., emoji sequences, combining chars)
    /// Uses Arc for cheap cloning
    Grapheme(Arc<str>),
}

impl Default for CellContent {
    fn default() -> Self {
        CellContent::Char(' ')
    }
}

impl CellContent {
    /// Create content from a single character
    pub fn from_char(c: char) -> Self {
        CellContent::Char(c)
    }

    /// Create content from a string (grapheme cluster)
    pub fn from_str(s: &str) -> Self {
        let mut chars = s.chars();
        if let Some(first) = chars.next() {
            if chars.next().is_none() {
                // Single character
                CellContent::Char(first)
            } else {
                // Multiple codepoints - grapheme cluster
                CellContent::Grapheme(Arc::from(s))
            }
        } else {
            CellContent::Char(' ')
        }
    }

    /// Get the content as a string slice
    pub fn as_str(&self) -> &str {
        match self {
            CellContent::Char(c) => {
                // This is a bit of a hack but avoids allocation
                // We rely on the caller to handle single chars appropriately
                unsafe {
                    std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                        c as *const char as *const u8,
                        0, // Empty slice - caller should use first_char() instead
                    ))
                }
            }
            CellContent::Grapheme(s) => s.as_ref(),
        }
    }

    /// Get the first (or only) character
    pub fn first_char(&self) -> char {
        match self {
            CellContent::Char(c) => *c,
            CellContent::Grapheme(s) => s.chars().next().unwrap_or(' '),
        }
    }

    /// Check if this is a space
    pub fn is_space(&self) -> bool {
        match self {
            CellContent::Char(' ') => true,
            CellContent::Grapheme(s) => s.as_ref() == " ",
            _ => false,
        }
    }

    /// Check if this is a single character
    pub fn is_single_char(&self) -> bool {
        matches!(self, CellContent::Char(_))
    }

    /// Get display width (using unicode-width)
    pub fn width(&self) -> usize {
        match self {
            CellContent::Char(c) => unicode_width::UnicodeWidthChar::width(*c).unwrap_or(1),
            CellContent::Grapheme(s) => unicode_width::UnicodeWidthStr::width(s.as_ref()),
        }
    }

    /// Convert to owned String
    pub fn to_string(&self) -> String {
        match self {
            CellContent::Char(c) => c.to_string(),
            CellContent::Grapheme(s) => s.to_string(),
        }
    }

    /// Check if this contains emoji
    pub fn is_emoji(&self) -> bool {
        let c = self.first_char();
        // Basic emoji detection - covers most emoji ranges
        matches!(c,
            '\u{1F300}'..='\u{1F9FF}' | // Misc Symbols, Emoticons, etc.
            '\u{2600}'..='\u{26FF}' |   // Misc Symbols
            '\u{2700}'..='\u{27BF}' |   // Dingbats
            '\u{1F600}'..='\u{1F64F}' | // Emoticons
            '\u{1F680}'..='\u{1F6FF}' | // Transport
            '\u{1F1E0}'..='\u{1F1FF}'   // Flags
        )
    }
}

/// Single terminal cell
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Cell content (char or grapheme cluster)
    content: CellContent,
    /// Foreground color
    pub fg: Color,
    /// Background color
    pub bg: Color,
    /// Cell flags (bold, italic, etc.)
    pub flags: CellFlags,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert 8-color ANSI code to RGB
    pub fn from_ansi(code: u8) -> Self {
        match code {
            0 => Self::rgb(0, 0, 0),       // Black
            1 => Self::rgb(170, 0, 0),     // Red
            2 => Self::rgb(0, 170, 0),     // Green
            3 => Self::rgb(170, 85, 0),    // Yellow
            4 => Self::rgb(0, 0, 170),     // Blue
            5 => Self::rgb(170, 0, 170),   // Magenta
            6 => Self::rgb(0, 170, 170),   // Cyan
            7 => Self::rgb(170, 170, 170), // White
            _ => Self::rgb(255, 255, 255),
        }
    }

    /// Convert 256-color code to RGB
    pub fn from_256(code: u8) -> Self {
        match code {
            0..=7 => Self::from_ansi(code),
            8..=15 => {
                // Bright colors
                let base = Self::from_ansi(code - 8);
                Self::rgb(
                    base.r.saturating_add(85),
                    base.g.saturating_add(85),
                    base.b.saturating_add(85),
                )
            }
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
            content: CellContent::Char(c),
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
        }
    }

    /// Create a new cell with a grapheme cluster
    pub fn from_grapheme(s: &str) -> Self {
        Self {
            content: CellContent::from_str(s),
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
        }
    }

    /// Create an empty cell
    pub fn empty() -> Self {
        Self {
            content: CellContent::Char(' '),
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
        }
    }

    /// Get the character (first char if grapheme)
    pub fn c(&self) -> char {
        self.content.first_char()
    }

    /// Set the cell content from a character
    pub fn set_char(&mut self, c: char) {
        self.content = CellContent::Char(c);
    }

    /// Set the cell content from a grapheme cluster
    pub fn set_grapheme(&mut self, s: &str) {
        self.content = CellContent::from_str(s);
    }

    /// Get the cell content
    pub fn content(&self) -> &CellContent {
        &self.content
    }

    /// Check if the cell is empty (space with no flags)
    pub fn is_empty(&self) -> bool {
        self.content.is_space() && self.flags.is_empty()
    }

    /// Check if the cell contains an emoji
    pub fn is_emoji(&self) -> bool {
        self.content.is_emoji()
    }

    /// Get the display width of the cell content
    pub fn width(&self) -> usize {
        self.content.width()
    }

    /// Reset the cell to empty
    pub fn reset(&mut self) {
        self.content = CellContent::Char(' ');
        self.fg = Color::rgb(255, 255, 255);
        self.bg = Color::rgb(0, 0, 0);
        self.flags = CellFlags::empty();
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
        assert_eq!(cell.c(), 'A');
        assert!(!cell.is_empty());
    }

    #[test]
    fn cell_empty() {
        let cell = Cell::empty();
        assert_eq!(cell.c(), ' ');
        assert!(cell.is_empty());
    }

    #[test]
    fn cell_reset() {
        let mut cell = Cell::new('X');
        cell.fg = Color::rgb(255, 0, 0);
        cell.flags = CellFlags::BOLD;
        cell.reset();
        assert_eq!(cell.c(), ' ');
        assert!(cell.flags.is_empty());
    }

    #[test]
    fn cell_grapheme() {
        let cell = Cell::from_grapheme("é"); // e + combining acute
        assert!(!cell.is_empty());
    }

    #[test]
    fn cell_emoji() {
        let cell = Cell::from_grapheme("👨‍👩‍👧");
        assert!(cell.is_emoji());
    }

    #[test]
    fn cell_content_single_char() {
        let content = CellContent::from_str("A");
        assert!(content.is_single_char());
        assert_eq!(content.first_char(), 'A');
    }

    #[test]
    fn cell_content_grapheme() {
        let content = CellContent::from_str("👨‍👩‍👧");
        assert!(!content.is_single_char());
        assert_eq!(content.to_string(), "👨‍👩‍👧");
    }

    #[test]
    fn cell_content_width() {
        let single = CellContent::from_char('A');
        assert_eq!(single.width(), 1);

        let wide = CellContent::from_char('中');
        assert_eq!(wide.width(), 2);
    }

    #[test]
    fn color_from_ansi() {
        let red = Color::from_ansi(1);
        assert_eq!(red.r, 170);
        assert_eq!(red.g, 0);
        assert_eq!(red.b, 0);
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
    fn cell_set_char() {
        let mut cell = Cell::empty();
        cell.set_char('Z');
        assert_eq!(cell.c(), 'Z');
    }

    #[test]
    fn cell_set_grapheme() {
        let mut cell = Cell::empty();
        cell.set_grapheme("🇺🇸");
        assert!(cell.is_emoji());
    }
}
