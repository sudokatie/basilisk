//! Terminal state combining grid, cursor, and parser

use unicode_segmentation::UnicodeSegmentation;
use crate::ansi::{Attr, Handler, Parser, Action, csi_dispatch, osc_dispatch};
use crate::render::sixel::SixelDecoder;
use super::cell::{CellFlags, Color};
use super::cursor::{Cursor, SavedCursor};
use super::grid::Grid;

/// Default foreground color (light gray)
const DEFAULT_FG: Color = Color { r: 204, g: 204, b: 204 };
/// Default background color (black)
const DEFAULT_BG: Color = Color { r: 0, g: 0, b: 0 };

/// Get the length of a UTF-8 sequence from its first byte
fn utf8_sequence_length(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte < 0xC0 {
        1 // Invalid start byte, treat as 1
    } else if byte < 0xE0 {
        2
    } else if byte < 0xF0 {
        3
    } else {
        4
    }
}

/// Item to process after UTF-8 decoding
#[derive(Clone, Copy)]
enum ProcessItem {
    Byte(u8),
    Char(char),
}

/// Sixel image placed in terminal
#[derive(Debug, Clone)]
pub struct SixelPlacement {
    /// Image data (RGBA)
    pub data: Vec<u8>,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Column position where image starts
    pub col: u16,
    /// Row position where image starts
    pub row: u16,
}

/// Terminal mode flags
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalModes {
    /// DECCKM - Application cursor keys
    pub application_cursor: bool,
    /// DECAWM - Auto-wrap mode
    pub auto_wrap: bool,
    /// DECOM - Origin mode (cursor relative to scroll region)
    pub origin_mode: bool,
    /// DECTCEM - Cursor visible
    pub cursor_visible: bool,
    /// Bracketed paste mode
    pub bracketed_paste: bool,
    /// Focus reporting
    pub focus_reporting: bool,
    /// Mouse tracking modes
    pub mouse_tracking: MouseMode,
}

/// Mouse tracking mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MouseMode {
    #[default]
    None,
    X10,           // Button press only
    Normal,        // Button press/release
    ButtonMotion,  // Press/release + motion while pressed
    AnyMotion,     // All motion events
    Sgr,           // SGR extended coordinates
}

/// Active hyperlink
#[derive(Debug, Clone)]
pub struct Hyperlink {
    pub id: Option<String>,
    pub url: String,
}

/// Complete terminal state
pub struct Terminal {
    grid: Grid,
    cursor: Cursor,
    parser: Parser,
    saved_cursor: Option<SavedCursor>,
    title: String,
    scroll_top: u16,
    scroll_bottom: u16,
    tab_stops: Vec<bool>,
    /// UTF-8 decoding buffer for incomplete sequences
    utf8_buffer: Vec<u8>,
    /// Terminal modes
    modes: TerminalModes,
    /// Sixel images in terminal
    sixel_images: Vec<SixelPlacement>,
    /// Sixel decoder for parsing DCS sequences
    sixel_decoder: SixelDecoder,
    /// Current hyperlink (OSC 8)
    current_hyperlink: Option<Hyperlink>,
    /// Working directory (OSC 7)
    working_directory: Option<String>,
}

impl Terminal {
    /// Create a new terminal with given dimensions
    pub fn new(cols: u16, rows: u16, scrollback: usize) -> Self {
        let mut tab_stops = vec![false; cols as usize];
        // Default tab stops every 8 columns
        for i in (0..cols as usize).step_by(8) {
            tab_stops[i] = true;
        }

        Self {
            grid: Grid::new(cols, rows, scrollback),
            cursor: Cursor::new(),
            parser: Parser::new(),
            saved_cursor: None,
            title: String::new(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            tab_stops,
            utf8_buffer: Vec::new(),
            modes: TerminalModes {
                cursor_visible: true,
                auto_wrap: true,
                ..Default::default()
            },
            sixel_images: Vec::new(),
            sixel_decoder: SixelDecoder::new(),
            current_hyperlink: None,
            working_directory: None,
        }
    }

    /// Get terminal modes
    pub fn modes(&self) -> &TerminalModes {
        &self.modes
    }

    /// Get mutable terminal modes
    pub fn modes_mut(&mut self) -> &mut TerminalModes {
        &mut self.modes
    }

    /// Get sixel images
    pub fn sixel_images(&self) -> &[SixelPlacement] {
        &self.sixel_images
    }

    /// Clear sixel images
    pub fn clear_sixel_images(&mut self) {
        self.sixel_images.clear();
    }

    /// Get current hyperlink
    pub fn current_hyperlink(&self) -> Option<&Hyperlink> {
        self.current_hyperlink.as_ref()
    }

    /// Get working directory
    pub fn working_directory(&self) -> Option<&str> {
        self.working_directory.as_deref()
    }

    /// Process input bytes with proper UTF-8 and grapheme cluster handling
    pub fn process(&mut self, bytes: &[u8]) {
        // Append new bytes to buffer for UTF-8 continuation handling
        self.utf8_buffer.extend_from_slice(bytes);
        
        // Collect characters to process (avoids borrow conflicts)
        let mut chars_to_process: Vec<ProcessItem> = Vec::new();
        let mut i = 0;
        
        while i < self.utf8_buffer.len() {
            let byte = self.utf8_buffer[i];
            
            // Check if this is an escape or control character
            if byte < 0x20 || byte == 0x1b || byte == 0x7f {
                chars_to_process.push(ProcessItem::Byte(byte));
                i += 1;
                continue;
            }
            
            // Try to decode UTF-8
            let seq_len = utf8_sequence_length(byte);
            if i + seq_len > self.utf8_buffer.len() {
                // Incomplete UTF-8 sequence, keep in buffer
                break;
            }
            
            // Decode and collect characters
            if let Ok(s) = std::str::from_utf8(&self.utf8_buffer[i..i + seq_len]) {
                for grapheme in s.graphemes(true) {
                    if let Some(c) = grapheme.chars().next() {
                        chars_to_process.push(ProcessItem::Char(c));
                    }
                }
            }
            
            i += seq_len;
        }
        
        // Drain processed bytes
        if i > 0 {
            self.utf8_buffer.drain(0..i);
        }
        
        // Now process collected items
        for item in chars_to_process {
            match item {
                ProcessItem::Byte(byte) => {
                    if let Some(action) = self.parser.advance(byte) {
                        self.execute(action);
                    }
                }
                ProcessItem::Char(c) => {
                    if let Some(action) = self.parser.advance_char(c) {
                        self.execute(action);
                    } else if self.parser.is_ground() {
                        self.input(c);
                    }
                }
            }
        }
    }
    
    /// Process input bytes (simple version for non-UTF8 contexts)
    pub fn process_raw(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if let Some(action) = self.parser.advance(byte) {
                self.execute(action);
            }
        }
    }

    /// Execute a parsed action
    fn execute(&mut self, action: Action) {
        match action {
            Action::Print(c) => self.input(c),
            Action::Execute(byte) => self.execute_control(byte),
            Action::CsiDispatch { params, intermediates, action } => {
                csi_dispatch(self, &params, &intermediates, action);
            }
            Action::OscDispatch(params) => {
                osc_dispatch(self, &params);
            }
            Action::DcsDispatch { params, intermediates, data } => {
                self.handle_dcs(&params, &intermediates, &data);
            }
        }
    }

    /// Execute a control character
    fn execute_control(&mut self, byte: u8) {
        match byte {
            0x07 => self.bell(),           // BEL
            0x08 => self.backspace(),      // BS
            0x09 => self.tab(),            // HT
            0x0A => self.linefeed(),       // LF
            0x0B => self.linefeed(),       // VT (treated as LF)
            0x0C => self.linefeed(),       // FF (treated as LF)
            0x0D => self.carriage_return(), // CR
            _ => {}
        }
    }

    /// Handle DCS (Device Control String) sequences
    fn handle_dcs(&mut self, params: &[u16], _intermediates: &[u8], data: &[u8]) {
        // Check if this is a sixel sequence (DCS P1;P2;P3 q ...)
        // The 'q' is typically consumed before we get here, so check data
        if data.is_empty() {
            return;
        }

        // Sixel sequences: params are aspect ratio hints, data contains sixel commands
        // Reset decoder for new image
        self.sixel_decoder.reset();
        
        // Decode sixel data
        self.sixel_decoder.decode(data);
        
        // Get the decoded image
        let image = self.sixel_decoder.image();
        
        if image.width > 0 && image.height > 0 {
            // Place image at current cursor position
            let placement = SixelPlacement {
                data: image.data.clone(),
                width: image.width,
                height: image.height,
                col: self.cursor.col,
                row: self.cursor.line,
            };
            
            self.sixel_images.push(placement);
            
            // Advance cursor past the image (sixel spec says cursor moves down)
            // Calculate how many rows the image occupies
            // Assuming cell_height is roughly 16 pixels (we'd need actual metrics)
            let cell_height = 16u32; // Default assumption
            let rows_needed = (image.height + cell_height - 1) / cell_height;
            
            for _ in 0..rows_needed {
                self.linefeed();
            }
        }
    }

    /// Get the grid
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Get the cursor
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// Get the window title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.grid.resize(cols, rows);
        self.scroll_bottom = rows.saturating_sub(1);
        
        // Resize tab stops
        self.tab_stops.resize(cols as usize, false);
        for i in (0..cols as usize).step_by(8) {
            self.tab_stops[i] = true;
        }

        // Clamp cursor
        self.cursor.col = self.cursor.col.min(cols.saturating_sub(1));
        self.cursor.line = self.cursor.line.min(rows.saturating_sub(1));
    }

    /// Write a character at cursor position, advancing cursor
    fn write_char(&mut self, c: char) {
        let cols = self.grid.cols();

        // Handle wide characters
        let width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) as u16;

        // Check if we're at the right margin and need to wrap
        // This is "pending wrap" - cursor at last col means next char triggers wrap
        if self.cursor.col >= cols {
            self.cursor.col = 0;
            if self.cursor.line >= self.scroll_bottom {
                self.scroll_up(1);
            } else {
                self.cursor.line += 1;
            }
        }

        // Write the character
        let cell = self.grid.cell_mut(self.cursor.col, self.cursor.line);
        cell.c = c;
        cell.fg = self.cursor.fg;
        cell.bg = self.cursor.bg;
        cell.flags = self.cursor.flags;

        if width > 1 {
            cell.flags |= CellFlags::WIDE;
            // Mark next cell as spacer
            if self.cursor.col + 1 < cols {
                let spacer = self.grid.cell_mut(self.cursor.col + 1, self.cursor.line);
                spacer.c = ' ';
                spacer.flags = CellFlags::WIDE_SPACER;
            }
        }

        // Advance cursor (allow it to go past last col, triggers wrap on next char)
        self.cursor.col += width;
    }
}

impl Handler for Terminal {
    fn input(&mut self, c: char) {
        self.write_char(c);
    }

    fn goto(&mut self, line: u16, col: u16) {
        self.cursor.line = line.min(self.grid.lines().saturating_sub(1));
        self.cursor.col = col.min(self.grid.cols().saturating_sub(1));
    }

    fn goto_line(&mut self, line: u16) {
        self.cursor.line = line.min(self.grid.lines().saturating_sub(1));
    }

    fn goto_col(&mut self, col: u16) {
        self.cursor.col = col.min(self.grid.cols().saturating_sub(1));
    }

    fn move_up(&mut self, n: u16) {
        self.cursor.move_up(n);
    }

    fn move_down(&mut self, n: u16) {
        self.cursor.move_down(n, self.grid.lines());
    }

    fn move_forward(&mut self, n: u16) {
        self.cursor.move_right(n, self.grid.cols());
    }

    fn move_backward(&mut self, n: u16) {
        self.cursor.move_left(n);
    }

    fn move_down_and_cr(&mut self, n: u16) {
        self.cursor.col = 0;
        self.cursor.move_down(n, self.grid.lines());
    }

    fn move_up_and_cr(&mut self, n: u16) {
        self.cursor.col = 0;
        self.cursor.move_up(n);
    }

    fn insert_blank(&mut self, n: u16) {
        let cols = self.grid.cols();
        let line = self.cursor.line;
        let col = self.cursor.col;

        // Shift characters right
        for c in (col..cols.saturating_sub(n)).rev() {
            let src = self.grid.cell(c, line).clone();
            *self.grid.cell_mut(c + n, line) = src;
        }

        // Clear inserted cells
        for c in col..(col + n).min(cols) {
            self.grid.cell_mut(c, line).reset();
        }
    }

    fn newline(&mut self) {
        self.linefeed();
    }

    fn carriage_return(&mut self) {
        self.cursor.carriage_return();
    }

    fn backspace(&mut self) {
        self.cursor.move_left(1);
    }

    fn tab(&mut self) {
        let cols = self.grid.cols() as usize;
        let mut col = self.cursor.col as usize + 1;
        
        while col < cols && !self.tab_stops.get(col).copied().unwrap_or(false) {
            col += 1;
        }
        
        self.cursor.col = (col as u16).min(self.grid.cols().saturating_sub(1));
    }

    fn erase_chars(&mut self, n: u16) {
        let cols = self.grid.cols();
        for c in self.cursor.col..(self.cursor.col + n).min(cols) {
            self.grid.cell_mut(c, self.cursor.line).reset();
        }
    }

    fn delete_chars(&mut self, n: u16) {
        let cols = self.grid.cols();
        let line = self.cursor.line;
        let col = self.cursor.col;

        // Shift characters left
        for c in col..cols.saturating_sub(n) {
            let src = self.grid.cell(c + n, line).clone();
            *self.grid.cell_mut(c, line) = src;
        }

        // Clear at end
        for c in cols.saturating_sub(n)..cols {
            self.grid.cell_mut(c, line).reset();
        }
    }

    fn erase_in_line(&mut self, mode: u16) {
        let cols = self.grid.cols();
        let line = self.cursor.line;

        let (start, end) = match mode {
            0 => (self.cursor.col, cols),        // Cursor to end
            1 => (0, self.cursor.col + 1),       // Start to cursor
            2 => (0, cols),                      // Entire line
            _ => return,
        };

        for c in start..end {
            self.grid.cell_mut(c, line).reset();
        }
    }

    fn erase_in_display(&mut self, mode: u16) {
        let cols = self.grid.cols();
        let rows = self.grid.lines();

        match mode {
            0 => {
                // Cursor to end of screen
                self.erase_in_line(0);
                for line in (self.cursor.line + 1)..rows {
                    self.grid.clear_line(line);
                }
            }
            1 => {
                // Start to cursor
                for line in 0..self.cursor.line {
                    self.grid.clear_line(line);
                }
                self.erase_in_line(1);
            }
            2 => {
                // Entire screen
                self.grid.clear();
            }
            3 => {
                // Entire screen + scrollback (not implemented)
                self.grid.clear();
            }
            _ => {}
        }
    }

    fn insert_lines(&mut self, n: u16) {
        let line = self.cursor.line;
        if line > self.scroll_bottom {
            return;
        }

        // Scroll region down
        for _ in 0..n {
            // Move lines down
            for l in (line..self.scroll_bottom).rev() {
                let row = self.grid.row(l).clone();
                *self.grid.row_mut(l + 1) = row;
            }
            self.grid.clear_line(line);
        }
    }

    fn delete_lines(&mut self, n: u16) {
        let line = self.cursor.line;
        if line > self.scroll_bottom {
            return;
        }

        // Scroll region up
        for _ in 0..n {
            // Move lines up
            for l in line..self.scroll_bottom {
                let row = self.grid.row(l + 1).clone();
                *self.grid.row_mut(l) = row;
            }
            self.grid.clear_line(self.scroll_bottom);
        }
    }

    fn clear_tab(&mut self, mode: u16) {
        match mode {
            0 => {
                // Clear tab at cursor
                if let Some(t) = self.tab_stops.get_mut(self.cursor.col as usize) {
                    *t = false;
                }
            }
            3 => {
                // Clear all tabs
                self.tab_stops.fill(false);
            }
            _ => {}
        }
    }

    fn set_attr(&mut self, attr: Attr) {
        match attr {
            Attr::Reset => {
                self.cursor.fg = DEFAULT_FG;
                self.cursor.bg = DEFAULT_BG;
                self.cursor.flags = CellFlags::empty();
            }
            Attr::Bold => self.cursor.flags |= CellFlags::BOLD,
            Attr::Dim => self.cursor.flags |= CellFlags::DIM,
            Attr::Italic => self.cursor.flags |= CellFlags::ITALIC,
            Attr::Underline => self.cursor.flags |= CellFlags::UNDERLINE,
            Attr::Blink => self.cursor.flags |= CellFlags::BLINK,
            Attr::Inverse => self.cursor.flags |= CellFlags::INVERSE,
            Attr::Hidden => self.cursor.flags |= CellFlags::HIDDEN,
            Attr::Strike => self.cursor.flags |= CellFlags::STRIKETHROUGH,
            Attr::CancelBold => self.cursor.flags.remove(CellFlags::BOLD),
            Attr::CancelDim => self.cursor.flags.remove(CellFlags::DIM),
            Attr::CancelItalic => self.cursor.flags.remove(CellFlags::ITALIC),
            Attr::CancelUnderline => self.cursor.flags.remove(CellFlags::UNDERLINE),
            Attr::CancelBlink => self.cursor.flags.remove(CellFlags::BLINK),
            Attr::CancelInverse => self.cursor.flags.remove(CellFlags::INVERSE),
            Attr::CancelHidden => self.cursor.flags.remove(CellFlags::HIDDEN),
            Attr::CancelStrike => self.cursor.flags.remove(CellFlags::STRIKETHROUGH),
            Attr::Foreground(c) => self.cursor.fg = c,
            Attr::Background(c) => self.cursor.bg = c,
            Attr::DefaultForeground => self.cursor.fg = DEFAULT_FG,
            Attr::DefaultBackground => self.cursor.bg = DEFAULT_BG,
        }
    }

    fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }

    fn bell(&mut self) {
        // Bell not implemented - would notify user
    }

    fn reset(&mut self) {
        self.cursor = Cursor::new();
        self.grid.clear();
        self.scroll_top = 0;
        self.scroll_bottom = self.grid.lines().saturating_sub(1);
        self.title.clear();
    }

    fn save_cursor(&mut self) {
        self.saved_cursor = Some(self.cursor.save());
    }

    fn restore_cursor(&mut self) {
        if let Some(saved) = &self.saved_cursor {
            self.cursor.restore(saved);
        }
    }

    fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let rows = self.grid.lines();
        self.scroll_top = top.saturating_sub(1).min(rows.saturating_sub(1));
        self.scroll_bottom = if bottom == 0 {
            rows.saturating_sub(1)
        } else {
            (bottom.saturating_sub(1)).min(rows.saturating_sub(1))
        };

        if self.scroll_top > self.scroll_bottom {
            std::mem::swap(&mut self.scroll_top, &mut self.scroll_bottom);
        }
    }

    fn scroll_up(&mut self, n: u16) {
        for _ in 0..n {
            self.grid.scroll_up(1);
        }
    }

    fn scroll_down(&mut self, n: u16) {
        for _ in 0..n {
            self.grid.scroll_down(1);
        }
    }

    fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor.visible = visible;
    }

    fn reverse_index(&mut self) {
        if self.cursor.line == self.scroll_top {
            self.scroll_down(1);
        } else {
            self.cursor.move_up(1);
        }
    }

    fn linefeed(&mut self) {
        if self.cursor.line >= self.scroll_bottom {
            self.scroll_up(1);
        } else {
            self.cursor.line += 1;
        }
    }

    fn set_mode(&mut self, mode: u16, enable: bool) {
        match mode {
            1 => self.modes.application_cursor = enable,    // DECCKM
            6 => self.modes.origin_mode = enable,           // DECOM
            7 => self.modes.auto_wrap = enable,             // DECAWM
            25 => {                                          // DECTCEM
                self.cursor.visible = enable;
                self.modes.cursor_visible = enable;
            }
            1000 => {                                        // X10 mouse
                self.modes.mouse_tracking = if enable { MouseMode::X10 } else { MouseMode::None };
            }
            1002 => {                                        // Button-event mouse
                self.modes.mouse_tracking = if enable { MouseMode::ButtonMotion } else { MouseMode::None };
            }
            1003 => {                                        // Any-event mouse
                self.modes.mouse_tracking = if enable { MouseMode::AnyMotion } else { MouseMode::None };
            }
            1006 => {                                        // SGR mouse mode
                if enable {
                    self.modes.mouse_tracking = MouseMode::Sgr;
                }
            }
            1004 => self.modes.focus_reporting = enable,    // Focus events
            2004 => self.modes.bracketed_paste = enable,    // Bracketed paste
            _ => {} // Unknown mode, ignore
        }
    }

    fn set_hyperlink(&mut self, id: Option<&str>, url: Option<&str>) {
        match url {
            Some(url) if !url.is_empty() => {
                self.current_hyperlink = Some(Hyperlink {
                    id: id.map(|s| s.to_string()),
                    url: url.to_string(),
                });
            }
            _ => {
                self.current_hyperlink = None;
            }
        }
    }

    fn set_working_directory(&mut self, path: &str) {
        self.working_directory = Some(path.to_string());
    }

    fn clipboard(&mut self, _clipboard: char, _data: Option<&str>) {
        // Clipboard operations are handled at the application level
        // Terminal just receives the request but doesn't act on it directly
        // for security reasons (per spec: "many terminals disable this by default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_new() {
        let term = Terminal::new(80, 24, 1000);
        assert_eq!(term.grid.cols(), 80);
        assert_eq!(term.grid.lines(), 24);
    }

    #[test]
    fn terminal_write_char() {
        let mut term = Terminal::new(80, 24, 1000);
        term.input('A');
        assert_eq!(term.grid.cell(0, 0).c, 'A');
        assert_eq!(term.cursor.col, 1);
    }

    #[test]
    fn terminal_newline() {
        let mut term = Terminal::new(80, 24, 1000);
        term.linefeed();
        assert_eq!(term.cursor.line, 1);
    }

    #[test]
    fn terminal_carriage_return() {
        let mut term = Terminal::new(80, 24, 1000);
        term.cursor.col = 10;
        term.carriage_return();
        assert_eq!(term.cursor.col, 0);
    }

    #[test]
    fn terminal_process_text() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"Hello");
        assert_eq!(term.grid.cell(0, 0).c, 'H');
        assert_eq!(term.grid.cell(4, 0).c, 'o');
        assert_eq!(term.cursor.col, 5);
    }

    #[test]
    fn terminal_process_csi_cursor() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"\x1b[10;20H"); // Move to line 10, col 20
        assert_eq!(term.cursor.line, 9);
        assert_eq!(term.cursor.col, 19);
    }

    #[test]
    fn terminal_process_csi_erase() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"ABCDE");
        term.process(b"\x1b[H\x1b[2J"); // Home and clear screen
        assert_eq!(term.grid.cell(0, 0).c, ' ');
    }

    #[test]
    fn terminal_process_sgr() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"\x1b[1;31mRed"); // Bold red
        assert!(term.cursor.flags.contains(CellFlags::BOLD));
        assert_eq!(term.grid.cell(0, 0).c, 'R');
    }

    #[test]
    fn terminal_tab() {
        let mut term = Terminal::new(80, 24, 1000);
        term.tab();
        assert_eq!(term.cursor.col, 8);
    }

    #[test]
    fn terminal_save_restore_cursor() {
        let mut term = Terminal::new(80, 24, 1000);
        term.cursor.col = 10;
        term.cursor.line = 5;
        term.save_cursor();

        term.cursor.col = 0;
        term.cursor.line = 0;

        term.restore_cursor();
        assert_eq!(term.cursor.col, 10);
        assert_eq!(term.cursor.line, 5);
    }

    #[test]
    fn terminal_scroll_at_bottom() {
        let mut term = Terminal::new(80, 5, 1000);
        term.cursor.line = 4; // Last line
        term.linefeed();
        // Should scroll, cursor stays at bottom
        assert_eq!(term.cursor.line, 4);
        assert_eq!(term.grid.scrollback_len(), 1);
    }

    #[test]
    fn terminal_line_wrap() {
        let mut term = Terminal::new(5, 2, 1000);
        term.process(b"ABCDE"); // Fill first line
        assert_eq!(term.cursor.col, 5); // Past last column (wrap pending)
        term.process(b"F"); // Should wrap
        assert_eq!(term.cursor.line, 1);
        assert_eq!(term.cursor.col, 1);
    }
}
