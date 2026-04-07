//! Cursor state and movement

use std::time::{Duration, Instant};
use super::cell::{Color, CellFlags};

/// Default blink interval in milliseconds
const DEFAULT_BLINK_INTERVAL_MS: u64 = 530;

/// Cursor shape
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Beam,
}

/// Cursor blink state
#[derive(Clone, Debug)]
pub struct CursorBlink {
    /// Whether blinking is enabled for the cursor
    pub enabled: bool,
    /// Current visibility state (toggled by timer)
    pub visible: bool,
    /// Blink interval
    pub interval: Duration,
    /// Last toggle time
    pub last_toggle: Instant,
}

impl Default for CursorBlink {
    fn default() -> Self {
        Self {
            enabled: true,
            visible: true,
            interval: Duration::from_millis(DEFAULT_BLINK_INTERVAL_MS),
            last_toggle: Instant::now(),
        }
    }
}

impl CursorBlink {
    /// Create new blink state with blinking enabled
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Default::default()
        }
    }

    /// Update blink state based on elapsed time
    /// Returns true if visibility changed
    pub fn update(&mut self) -> bool {
        if !self.enabled {
            // Ensure visible when blinking disabled
            if !self.visible {
                self.visible = true;
                return true;
            }
            return false;
        }

        let now = Instant::now();
        if now.duration_since(self.last_toggle) >= self.interval {
            self.visible = !self.visible;
            self.last_toggle = now;
            true
        } else {
            false
        }
    }

    /// Reset blink state (make visible and reset timer)
    pub fn reset(&mut self) {
        self.visible = true;
        self.last_toggle = Instant::now();
    }

    /// Check if cursor should be drawn (considering blink state)
    pub fn should_draw(&self) -> bool {
        !self.enabled || self.visible
    }
}

/// Saved cursor state for ESC 7 / ESC 8
#[derive(Clone, Debug)]
pub struct SavedCursor {
    pub col: u16,
    pub line: u16,
    pub fg: Color,
    pub bg: Color,
    pub flags: CellFlags,
}

/// Terminal cursor
#[derive(Clone, Debug)]
pub struct Cursor {
    pub col: u16,
    pub line: u16,
    pub fg: Color,
    pub bg: Color,
    pub flags: CellFlags,
    pub visible: bool,
    pub shape: CursorShape,
    pub blink: CursorBlink,
}

impl Default for Cursor {
    fn default() -> Self {
        Self::new()
    }
}

impl Cursor {
    pub fn new() -> Self {
        Self {
            col: 0,
            line: 0,
            fg: Color::rgb(255, 255, 255),
            bg: Color::rgb(0, 0, 0),
            flags: CellFlags::empty(),
            visible: true,
            shape: CursorShape::Block,
            blink: CursorBlink::default(),
        }
    }

    /// Set cursor shape from DECSCUSR parameter
    /// 0, 1 = blink block, 2 = steady block
    /// 3 = blink underline, 4 = steady underline
    /// 5 = blink bar, 6 = steady bar
    pub fn set_shape_decscusr(&mut self, param: u16) {
        let (shape, blink) = match param {
            0 | 1 => (CursorShape::Block, true),
            2 => (CursorShape::Block, false),
            3 => (CursorShape::Underline, true),
            4 => (CursorShape::Underline, false),
            5 => (CursorShape::Beam, true),
            6 => (CursorShape::Beam, false),
            _ => (CursorShape::Block, true), // Default
        };
        self.shape = shape;
        self.blink.enabled = blink;
        self.blink.reset();
    }

    /// Check if cursor should be drawn (considering visibility and blink)
    pub fn should_draw(&self) -> bool {
        self.visible && self.blink.should_draw()
    }

    /// Update blink state
    pub fn update_blink(&mut self) -> bool {
        self.blink.update()
    }

    /// Reset blink timer (call on keypress)
    pub fn reset_blink(&mut self) {
        self.blink.reset();
    }

    pub fn move_to(&mut self, col: u16, line: u16) {
        self.col = col;
        self.line = line;
    }

    pub fn move_up(&mut self, n: u16) {
        self.line = self.line.saturating_sub(n);
    }

    pub fn move_down(&mut self, n: u16, max: u16) {
        self.line = (self.line + n).min(max.saturating_sub(1));
    }

    pub fn move_left(&mut self, n: u16) {
        self.col = self.col.saturating_sub(n);
    }

    pub fn move_right(&mut self, n: u16, max: u16) {
        self.col = (self.col + n).min(max.saturating_sub(1));
    }

    pub fn carriage_return(&mut self) {
        self.col = 0;
    }

    pub fn save(&self) -> SavedCursor {
        SavedCursor {
            col: self.col,
            line: self.line,
            fg: self.fg,
            bg: self.bg,
            flags: self.flags,
        }
    }

    pub fn restore(&mut self, saved: &SavedCursor) {
        self.col = saved.col;
        self.line = saved.line;
        self.fg = saved.fg;
        self.bg = saved.bg;
        self.flags = saved.flags;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_new() {
        let cursor = Cursor::new();
        assert_eq!(cursor.col, 0);
        assert_eq!(cursor.line, 0);
        assert!(cursor.visible);
    }

    #[test]
    fn cursor_move_to() {
        let mut cursor = Cursor::new();
        cursor.move_to(10, 5);
        assert_eq!(cursor.col, 10);
        assert_eq!(cursor.line, 5);
    }

    #[test]
    fn cursor_move_up() {
        let mut cursor = Cursor::new();
        cursor.line = 10;
        cursor.move_up(3);
        assert_eq!(cursor.line, 7);
    }

    #[test]
    fn cursor_move_up_bounds() {
        let mut cursor = Cursor::new();
        cursor.line = 2;
        cursor.move_up(10);
        assert_eq!(cursor.line, 0);
    }

    #[test]
    fn cursor_move_down() {
        let mut cursor = Cursor::new();
        cursor.move_down(5, 24);
        assert_eq!(cursor.line, 5);
    }

    #[test]
    fn cursor_move_down_bounds() {
        let mut cursor = Cursor::new();
        cursor.move_down(100, 24);
        assert_eq!(cursor.line, 23);
    }

    #[test]
    fn cursor_move_left() {
        let mut cursor = Cursor::new();
        cursor.col = 10;
        cursor.move_left(3);
        assert_eq!(cursor.col, 7);
    }

    #[test]
    fn cursor_move_right() {
        let mut cursor = Cursor::new();
        cursor.move_right(5, 80);
        assert_eq!(cursor.col, 5);
    }

    #[test]
    fn cursor_move_right_bounds() {
        let mut cursor = Cursor::new();
        cursor.move_right(100, 80);
        assert_eq!(cursor.col, 79);
    }

    #[test]
    fn cursor_carriage_return() {
        let mut cursor = Cursor::new();
        cursor.col = 50;
        cursor.carriage_return();
        assert_eq!(cursor.col, 0);
    }

    #[test]
    fn cursor_save_restore() {
        let mut cursor = Cursor::new();
        cursor.col = 10;
        cursor.line = 5;
        cursor.fg = Color::rgb(255, 0, 0);
        cursor.flags = CellFlags::BOLD;

        let saved = cursor.save();

        cursor.col = 0;
        cursor.line = 0;
        cursor.fg = Color::rgb(0, 0, 0);
        cursor.flags = CellFlags::empty();

        cursor.restore(&saved);
        assert_eq!(cursor.col, 10);
        assert_eq!(cursor.line, 5);
        assert_eq!(cursor.fg, Color::rgb(255, 0, 0));
        assert!(cursor.flags.contains(CellFlags::BOLD));
    }
}
