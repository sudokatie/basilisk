//! Escape sequence handler trait

use crate::term::cell::Color;

/// Terminal attribute for SGR sequences
#[derive(Debug, Clone, PartialEq)]
pub enum Attr {
    Reset,
    Bold,
    Dim,
    Italic,
    Underline,
    Blink,
    Inverse,
    Hidden,
    Strike,
    CancelBold,
    CancelDim,
    CancelItalic,
    CancelUnderline,
    CancelBlink,
    CancelInverse,
    CancelHidden,
    CancelStrike,
    Foreground(Color),
    Background(Color),
    DefaultForeground,
    DefaultBackground,
}

/// Handler trait for terminal escape sequences
pub trait Handler {
    /// Print a character at cursor position
    fn input(&mut self, c: char);

    /// Move cursor to absolute position (1-indexed in ANSI, convert to 0-indexed)
    fn goto(&mut self, line: u16, col: u16);

    /// Move cursor up by n lines
    fn goto_line(&mut self, line: u16);

    /// Move cursor to column
    fn goto_col(&mut self, col: u16);

    /// Move cursor up
    fn move_up(&mut self, n: u16);

    /// Move cursor down
    fn move_down(&mut self, n: u16);

    /// Move cursor forward (right)
    fn move_forward(&mut self, n: u16);

    /// Move cursor backward (left)
    fn move_backward(&mut self, n: u16);

    /// Move cursor down and to column 1
    fn move_down_and_cr(&mut self, n: u16);

    /// Move cursor up and to column 1
    fn move_up_and_cr(&mut self, n: u16);

    /// Insert blank characters at cursor
    fn insert_blank(&mut self, n: u16);

    /// Newline
    fn newline(&mut self);

    /// Carriage return
    fn carriage_return(&mut self);

    /// Backspace
    fn backspace(&mut self);

    /// Horizontal tab
    fn tab(&mut self);

    /// Erase characters at cursor
    fn erase_chars(&mut self, n: u16);

    /// Delete characters at cursor (shift left)
    fn delete_chars(&mut self, n: u16);

    /// Erase in line: 0=cursor to end, 1=start to cursor, 2=entire line
    fn erase_in_line(&mut self, mode: u16);

    /// Erase in display: 0=cursor to end, 1=start to cursor, 2=entire screen, 3=scrollback
    fn erase_in_display(&mut self, mode: u16);

    /// Insert blank lines at cursor
    fn insert_lines(&mut self, n: u16);

    /// Delete lines at cursor
    fn delete_lines(&mut self, n: u16);

    /// Clear tab stop
    fn clear_tab(&mut self, mode: u16);

    /// Set terminal attribute
    fn set_attr(&mut self, attr: Attr);

    /// Set window title
    fn set_title(&mut self, title: &str);

    /// Bell
    fn bell(&mut self);

    /// Reset terminal state
    fn reset(&mut self);

    /// Save cursor position
    fn save_cursor(&mut self);

    /// Restore cursor position
    fn restore_cursor(&mut self);

    /// Set scrolling region (top and bottom lines, 1-indexed)
    fn set_scroll_region(&mut self, top: u16, bottom: u16);

    /// Scroll up by n lines
    fn scroll_up(&mut self, n: u16);

    /// Scroll down by n lines
    fn scroll_down(&mut self, n: u16);

    /// Set cursor visible
    fn set_cursor_visible(&mut self, visible: bool);

    /// Reverse index (scroll down if at top)
    fn reverse_index(&mut self);

    /// Line feed (may scroll)
    fn linefeed(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_variants() {
        let attr = Attr::Bold;
        assert_eq!(attr, Attr::Bold);

        let fg = Attr::Foreground(Color::rgb(255, 0, 0));
        match fg {
            Attr::Foreground(c) => assert_eq!(c.r, 255),
            _ => panic!("Expected Foreground"),
        }
    }

    #[test]
    fn attr_reset() {
        let attr = Attr::Reset;
        assert_eq!(attr, Attr::Reset);
    }
}
