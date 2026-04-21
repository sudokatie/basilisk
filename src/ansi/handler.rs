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
    /// True color foreground (RGB)
    Foreground(Color),
    /// True color background (RGB)
    Background(Color),
    /// ANSI indexed foreground (0-255, uses palette for 0-15)
    ForegroundIndex(u8),
    /// ANSI indexed background (0-255, uses palette for 0-15)
    BackgroundIndex(u8),
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

    /// Set terminal mode (DEC private modes)
    /// Common modes:
    /// - 1: DECCKM (application cursor keys)
    /// - 6: DECOM (origin mode)
    /// - 7: DECAWM (auto-wrap)
    /// - 25: DECTCEM (cursor visible)
    /// - 1000: X10 mouse tracking
    /// - 1002: Button-event mouse tracking
    /// - 1003: Any-event mouse tracking
    /// - 1006: SGR mouse mode
    /// - 2004: Bracketed paste
    fn set_mode(&mut self, mode: u16, enable: bool);

    /// Set hyperlink (OSC 8)
    fn set_hyperlink(&mut self, id: Option<&str>, url: Option<&str>);

    /// Set working directory (OSC 7)
    fn set_working_directory(&mut self, path: &str);

    /// Clipboard operation (OSC 52)
    fn clipboard(&mut self, clipboard: char, data: Option<&str>);

    /// DECRQSS - Request Selection or Setting
    /// Returns the response bytes to send back, or None if not supported
    /// query is the parameter string (e.g., "m" for SGR, "r" for DECSTBM, "\"p" for DECSCL)
    fn decrqss(&mut self, _query: &str) -> Option<Vec<u8>> {
        None // Default: not supported
    }

    /// Write response back to the PTY (for queries like DECRQSS, DA, etc.)
    fn write_response(&mut self, _response: &[u8]) {
        // Default: do nothing (terminal implementations should override)
    }

    /// Set tab stop at current cursor position (HTS - ESC H)
    fn set_tab_stop(&mut self) {}

    /// Set cursor shape (DECSCUSR)
    /// 0,1 = block, 2 = block (not blinking), 3 = underline, 4 = underline (not blinking),
    /// 5 = bar, 6 = bar (not blinking)
    fn set_cursor_shape(&mut self, _shape: u16) {}

    /// Primary Device Attributes (DA1) - CSI c or CSI 0 c
    /// Terminal should respond with its capabilities
    fn primary_device_attributes(&mut self) {}

    /// Secondary Device Attributes (DA2) - CSI > c
    fn secondary_device_attributes(&mut self) {}

    /// Tertiary Device Attributes (DA3) - CSI = c
    fn tertiary_device_attributes(&mut self) {}

    /// Device Status Report (DSR) - CSI n
    /// mode 5 = status report, mode 6 = cursor position report
    fn device_status_report(&mut self, _mode: u16) {}

    /// Designate character set G0 (ESC ( X)
    fn designate_g0(&mut self, _charset: char) {}

    /// Designate character set G1 (ESC ) X)
    fn designate_g1(&mut self, _charset: char) {}

    /// Shift to G0 (SI - 0x0F)
    fn shift_in(&mut self) {}

    /// Shift to G1 (SO - 0x0E)
    fn shift_out(&mut self) {}

    /// Soft reset (DECSTR) - CSI ! p
    fn soft_reset(&mut self) {}

    /// Request terminal parameters (DECREQTPARM) - CSI x
    fn request_terminal_parameters(&mut self, _mode: u16) {}

    /// Write data back to PTY (for terminal responses)
    fn write_to_pty(&mut self, _data: &[u8]) {}
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
