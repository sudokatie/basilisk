//! Terminal cell representation

use bitflags::bitflags;

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

/// Single terminal cell
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
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
    pub fn new(c: char) -> Self {
        Self {
            c,
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
        }
    }

    pub fn empty() -> Self {
        Self {
            c: ' ',
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.c == ' ' && self.flags.is_empty()
    }

    pub fn reset(&mut self) {
        self.c = ' ';
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
        assert_eq!(cell.c, 'A');
        assert!(!cell.is_empty());
    }

    #[test]
    fn cell_empty() {
        let cell = Cell::empty();
        assert_eq!(cell.c, ' ');
        assert!(cell.is_empty());
    }

    #[test]
    fn cell_reset() {
        let mut cell = Cell::new('X');
        cell.fg = Color::rgb(255, 0, 0);
        cell.flags = CellFlags::BOLD;
        cell.reset();
        assert_eq!(cell.c, ' ');
        assert!(cell.flags.is_empty());
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
}
