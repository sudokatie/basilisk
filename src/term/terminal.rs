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
    /// DECKPAM/DECKPNM - Application keypad mode
    pub application_keypad: bool,
    /// DECAWM - Auto-wrap mode
    pub auto_wrap: bool,
    /// DECOM - Origin mode (cursor relative to scroll region)
    pub origin_mode: bool,
    /// DECTCEM - Cursor visible
    pub cursor_visible: bool,
    /// LNM - Line feed/new line mode
    pub linefeed_mode: bool,
    /// Bracketed paste mode
    pub bracketed_paste: bool,
    /// Focus reporting
    pub focus_reporting: bool,
    /// Mouse tracking modes
    pub mouse_tracking: MouseMode,
    /// Alternate screen active
    pub alternate_screen: bool,
    /// Save cursor on alternate screen switch
    pub save_cursor_on_switch: bool,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hyperlink {
    pub id: Option<String>,
    pub url: String,
}

/// Bell event for notification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BellEvent {
    /// Visual bell - flash the screen
    Visual,
    /// Audible bell - system beep
    Audible,
    /// Both visual and audible
    Both,
}

/// Complete terminal state
pub struct Terminal {
    /// Primary screen grid
    grid: Grid,
    /// Alternate screen grid (for vim, less, etc.)
    alternate_grid: Option<Grid>,
    /// Current cursor
    cursor: Cursor,
    /// Saved cursor for primary screen
    saved_cursor: Option<SavedCursor>,
    /// Saved cursor for alternate screen
    alternate_saved_cursor: Option<SavedCursor>,
    /// Parser state
    parser: Parser,
    /// Window title
    title: String,
    /// Icon name
    icon_name: String,
    /// Scroll region top
    scroll_top: u16,
    /// Scroll region bottom
    scroll_bottom: u16,
    /// Tab stops
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
    /// Hyperlink storage per cell (col, line) -> hyperlink
    cell_hyperlinks: std::collections::HashMap<(u16, u16), Hyperlink>,
    /// Working directory (OSC 7)
    working_directory: Option<String>,
    /// Pending response to send back to PTY (e.g., from DECRQSS)
    pending_response: Option<Vec<u8>>,
    /// Pending bell event
    pending_bell: Option<BellEvent>,
    /// Scrollback viewport offset (0 = live view, >0 = viewing history)
    viewport_offset: usize,
    /// Terminal dimensions
    cols: u16,
    rows: u16,
    /// Scrollback limit
    scrollback_limit: usize,
    /// Configurable color palette (ANSI 0-255)
    color_palette: Option<Vec<Color>>,
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
            alternate_grid: None,
            cursor: Cursor::new(),
            saved_cursor: None,
            alternate_saved_cursor: None,
            parser: Parser::new(),
            title: String::new(),
            icon_name: String::new(),
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
            cell_hyperlinks: std::collections::HashMap::new(),
            working_directory: None,
            pending_response: None,
            pending_bell: None,
            viewport_offset: 0,
            cols,
            rows,
            scrollback_limit: scrollback,
            color_palette: None,
        }
    }

    /// Set custom color palette
    pub fn set_color_palette(&mut self, palette: Vec<Color>) {
        self.color_palette = Some(palette);
    }

    /// Get color from palette or default
    pub fn get_palette_color(&self, index: u8) -> Color {
        if let Some(ref palette) = self.color_palette {
            if (index as usize) < palette.len() {
                return palette[index as usize];
            }
        }
        Color::from_256(index)
    }

    /// Get terminal modes
    pub fn modes(&self) -> &TerminalModes {
        &self.modes
    }

    /// Get mutable terminal modes
    pub fn modes_mut(&mut self) -> &mut TerminalModes {
        &mut self.modes
    }

    /// Take pending response (if any) to send back to PTY
    pub fn take_pending_response(&mut self) -> Option<Vec<u8>> {
        self.pending_response.take()
    }

    /// Check if there's a pending response
    pub fn has_pending_response(&self) -> bool {
        self.pending_response.is_some()
    }

    /// Take pending bell event
    pub fn take_pending_bell(&mut self) -> Option<BellEvent> {
        self.pending_bell.take()
    }

    /// Check if there's a pending bell
    pub fn has_pending_bell(&self) -> bool {
        self.pending_bell.is_some()
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

    /// Get hyperlink at cell position
    pub fn get_cell_hyperlink(&self, col: u16, line: u16) -> Option<&Hyperlink> {
        self.cell_hyperlinks.get(&(col, line))
    }

    /// Get working directory
    pub fn working_directory(&self) -> Option<&str> {
        self.working_directory.as_deref()
    }

    /// Check if viewing scrollback (not live)
    pub fn is_viewing_scrollback(&self) -> bool {
        self.viewport_offset > 0
    }

    /// Get viewport offset
    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    /// Scroll viewport up (into history)
    pub fn scroll_viewport_up(&mut self, lines: usize) {
        let max_offset = self.grid.scrollback_len();
        self.viewport_offset = (self.viewport_offset + lines).min(max_offset);
    }

    /// Scroll viewport down (toward live)
    pub fn scroll_viewport_down(&mut self, lines: usize) {
        self.viewport_offset = self.viewport_offset.saturating_sub(lines);
    }

    /// Reset viewport to live view
    pub fn reset_viewport(&mut self) {
        self.viewport_offset = 0;
    }

    /// Scroll viewport to top of scrollback
    pub fn scroll_viewport_to_top(&mut self) {
        self.viewport_offset = self.grid.scrollback_len();
    }

    /// Scroll viewport to bottom (live)
    pub fn scroll_viewport_to_bottom(&mut self) {
        self.viewport_offset = 0;
    }

    /// Check if on alternate screen
    pub fn is_alternate_screen(&self) -> bool {
        self.modes.alternate_screen
    }

    /// Switch to alternate screen buffer
    fn switch_to_alternate(&mut self) {
        if self.modes.alternate_screen {
            return; // Already on alternate
        }

        // Save primary cursor
        self.saved_cursor = Some(self.cursor.save());

        // Create alternate grid if needed (no scrollback)
        if self.alternate_grid.is_none() {
            self.alternate_grid = Some(Grid::new(self.cols, self.rows, 0));
        }

        // Swap grids
        if let Some(ref mut alt) = self.alternate_grid {
            std::mem::swap(&mut self.grid, alt);
        }

        // Clear alternate screen
        self.grid.clear();

        // Reset cursor position
        self.cursor = Cursor::new();

        // Reset scroll region
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);

        // Reset viewport
        self.viewport_offset = 0;

        self.modes.alternate_screen = true;
    }

    /// Switch back to primary screen buffer
    fn switch_to_primary(&mut self) {
        if !self.modes.alternate_screen {
            return; // Already on primary
        }

        // Save alternate cursor
        self.alternate_saved_cursor = Some(self.cursor.save());

        // Swap grids back
        if let Some(ref mut alt) = self.alternate_grid {
            std::mem::swap(&mut self.grid, alt);
        }

        // Restore primary cursor
        if let Some(ref saved) = self.saved_cursor {
            self.cursor.restore(saved);
        }

        // Reset scroll region for primary
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);

        self.modes.alternate_screen = false;
    }

    /// Process input bytes with proper UTF-8 and grapheme cluster handling
    pub fn process(&mut self, bytes: &[u8]) {
        // Reset viewport to live when receiving input
        if self.viewport_offset > 0 {
            self.viewport_offset = 0;
        }

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
            0x0E => {},                    // SO - shift out (ignored)
            0x0F => {},                    // SI - shift in (ignored)
            // ESC sequences that come through as execute
            b'7' => self.save_cursor(),    // DECSC
            b'8' => self.restore_cursor(), // DECRC
            b'c' => self.reset(),          // RIS - full reset
            b'D' => self.linefeed(),       // IND - index
            b'E' => {                      // NEL - next line
                self.carriage_return();
                self.linefeed();
            }
            b'M' => self.reverse_index(),  // RI - reverse index
            b'=' => {                      // DECKPAM - keypad application mode
                self.modes.application_keypad = true;
            }
            b'>' => {                      // DECKPNM - keypad numeric mode
                self.modes.application_keypad = false;
            }
            _ => {}
        }
    }

    /// Handle DCS (Device Control String) sequences
    fn handle_dcs(&mut self, params: &[u16], intermediates: &[u8], data: &[u8]) {
        if data.is_empty() {
            return;
        }

        // Check for DECRQSS (DCS $ q Pt ST) - Request Status String
        if intermediates.contains(&b'$') {
            if let Ok(query) = std::str::from_utf8(data) {
                let query = query.trim_end_matches(|c| c == '\x1b' || c == '\\');
                if let Some(response) = self.decrqss(query) {
                    self.pending_response = Some(response);
                }
            }
            return;
        }

        // Sixel sequences
        self.sixel_decoder.reset();
        self.sixel_decoder.decode(data);
        
        let image = self.sixel_decoder.image();
        
        if image.width > 0 && image.height > 0 {
            let placement = SixelPlacement {
                data: image.data.clone(),
                width: image.width,
                height: image.height,
                col: self.cursor.col,
                row: self.cursor.line,
            };
            
            self.sixel_images.push(placement);
            
            let cell_height = 16u32;
            let rows_needed = (image.height + cell_height - 1) / cell_height;
            
            for _ in 0..rows_needed {
                self.linefeed();
            }
        }
    }

    /// Get the grid (returns visible grid based on viewport)
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Get mutable grid
    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    /// Get the cursor
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// Get mutable cursor
    pub fn cursor_mut(&mut self) -> &mut Cursor {
        &mut self.cursor
    }

    /// Get the window title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Get the icon name
    pub fn icon_name(&self) -> &str {
        &self.icon_name
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;

        self.grid.resize(cols, rows);
        
        // Resize alternate grid if it exists
        if let Some(ref mut alt) = self.alternate_grid {
            alt.resize(cols, rows);
        }

        self.scroll_bottom = rows.saturating_sub(1);
        
        // Resize tab stops
        self.tab_stops.resize(cols as usize, false);
        for i in (0..cols as usize).step_by(8) {
            self.tab_stops[i] = true;
        }

        // Clamp cursor
        self.cursor.col = self.cursor.col.min(cols.saturating_sub(1));
        self.cursor.line = self.cursor.line.min(rows.saturating_sub(1));

        // Reset viewport
        self.viewport_offset = 0;
    }

    /// Write a character at cursor position, advancing cursor
    fn write_char(&mut self, c: char) {
        self.write_grapheme_char(c, None);
    }

    /// Write a grapheme cluster at cursor position
    pub fn write_grapheme(&mut self, grapheme: &str) {
        let mut chars = grapheme.chars();
        if let Some(first) = chars.next() {
            if chars.next().is_none() {
                self.write_grapheme_char(first, None);
            } else {
                self.write_grapheme_char(grapheme.chars().next().unwrap(), Some(grapheme));
            }
        }
    }

    /// Internal: write a character or grapheme cluster
    fn write_grapheme_char(&mut self, c: char, grapheme: Option<&str>) {
        let cols = self.grid.cols();

        // Handle wide characters
        let width = if let Some(g) = grapheme {
            unicode_width::UnicodeWidthStr::width(g) as u16
        } else {
            unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) as u16
        };

        // Check if we're at the right margin and need to wrap
        if self.cursor.col >= cols {
            if self.modes.auto_wrap {
                self.cursor.col = 0;
                if self.cursor.line >= self.scroll_bottom {
                    self.scroll_up(1);
                } else {
                    self.cursor.line += 1;
                }
            } else {
                self.cursor.col = cols.saturating_sub(1);
            }
        }

        // Write the character or grapheme
        let cell = self.grid.cell_mut(self.cursor.col, self.cursor.line);
        if let Some(g) = grapheme {
            cell.set_grapheme(g);
        } else {
            cell.set_char(c);
        }
        cell.fg = self.cursor.fg;
        cell.bg = self.cursor.bg;
        cell.flags = self.cursor.flags;

        // Store hyperlink for this cell if active
        if let Some(ref hyperlink) = self.current_hyperlink {
            self.cell_hyperlinks.insert(
                (self.cursor.col, self.cursor.line),
                hyperlink.clone()
            );
            // Mark cell as having hyperlink
            cell.flags |= CellFlags::UNDERLINE; // Visual indication
        }

        if width > 1 {
            cell.flags |= CellFlags::WIDE;
            if self.cursor.col + 1 < cols {
                let spacer = self.grid.cell_mut(self.cursor.col + 1, self.cursor.line);
                spacer.set_char(' ');
                spacer.flags = CellFlags::WIDE_SPACER;
            }
        }

        self.cursor.col += width;
    }

    /// Get cell for rendering at viewport position
    pub fn get_viewport_cell(&self, col: u16, viewport_row: u16) -> Option<&super::cell::Cell> {
        if self.viewport_offset == 0 {
            // Live view - just return grid cell
            if viewport_row < self.grid.lines() && col < self.grid.cols() {
                Some(self.grid.cell(col, viewport_row))
            } else {
                None
            }
        } else {
            // Viewing scrollback
            let scrollback_len = self.grid.scrollback_len();
            let grid_lines = self.grid.lines() as usize;
            let total_row = viewport_row as usize + (scrollback_len - self.viewport_offset);

            if total_row < scrollback_len {
                // Row is in scrollback
                self.grid.scrollback_row(scrollback_len - total_row - 1)
                    .and_then(|row| row.cells.get(col as usize))
            } else {
                // Row is in visible grid
                let grid_row = total_row - scrollback_len;
                if grid_row < grid_lines && (col as usize) < self.grid.cols() as usize {
                    Some(self.grid.cell(col, grid_row as u16))
                } else {
                    None
                }
            }
        }
    }
}

impl Handler for Terminal {
    fn input(&mut self, c: char) {
        self.write_char(c);
    }

    fn goto(&mut self, line: u16, col: u16) {
        let effective_line = if self.modes.origin_mode {
            self.scroll_top + line
        } else {
            line
        };
        self.cursor.line = effective_line.min(self.grid.lines().saturating_sub(1));
        self.cursor.col = col.min(self.grid.cols().saturating_sub(1));
    }

    fn goto_line(&mut self, line: u16) {
        let effective_line = if self.modes.origin_mode {
            self.scroll_top + line
        } else {
            line
        };
        self.cursor.line = effective_line.min(self.grid.lines().saturating_sub(1));
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

        for c in (col..cols.saturating_sub(n)).rev() {
            let src = self.grid.cell(c, line).clone();
            *self.grid.cell_mut(c + n, line) = src;
        }

        for c in col..(col + n).min(cols) {
            self.grid.cell_mut(c, line).reset();
        }
    }

    fn newline(&mut self) {
        if self.modes.linefeed_mode {
            self.carriage_return();
        }
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
            self.cell_hyperlinks.remove(&(c, self.cursor.line));
        }
    }

    fn delete_chars(&mut self, n: u16) {
        let cols = self.grid.cols();
        let line = self.cursor.line;
        let col = self.cursor.col;

        for c in col..cols.saturating_sub(n) {
            let src = self.grid.cell(c + n, line).clone();
            *self.grid.cell_mut(c, line) = src;
            // Move hyperlink if exists
            if let Some(link) = self.cell_hyperlinks.remove(&(c + n, line)) {
                self.cell_hyperlinks.insert((c, line), link);
            }
        }

        for c in cols.saturating_sub(n)..cols {
            self.grid.cell_mut(c, line).reset();
            self.cell_hyperlinks.remove(&(c, line));
        }
    }

    fn erase_in_line(&mut self, mode: u16) {
        let cols = self.grid.cols();
        let line = self.cursor.line;

        let (start, end) = match mode {
            0 => (self.cursor.col, cols),
            1 => (0, self.cursor.col + 1),
            2 => (0, cols),
            _ => return,
        };

        for c in start..end {
            self.grid.cell_mut(c, line).reset();
            self.cell_hyperlinks.remove(&(c, line));
        }
    }

    fn erase_in_display(&mut self, mode: u16) {
        let cols = self.grid.cols();
        let rows = self.grid.lines();

        match mode {
            0 => {
                self.erase_in_line(0);
                for line in (self.cursor.line + 1)..rows {
                    self.grid.clear_line(line);
                    for c in 0..cols {
                        self.cell_hyperlinks.remove(&(c, line));
                    }
                }
            }
            1 => {
                for line in 0..self.cursor.line {
                    self.grid.clear_line(line);
                    for c in 0..cols {
                        self.cell_hyperlinks.remove(&(c, line));
                    }
                }
                self.erase_in_line(1);
            }
            2 => {
                self.grid.clear();
                self.cell_hyperlinks.clear();
            }
            3 => {
                // Clear screen and scrollback
                self.grid.clear();
                self.cell_hyperlinks.clear();
                // Note: Grid doesn't expose clear_scrollback, would need to add
            }
            _ => {}
        }
    }

    fn insert_lines(&mut self, n: u16) {
        let line = self.cursor.line;
        if line > self.scroll_bottom {
            return;
        }

        for _ in 0..n {
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

        for _ in 0..n {
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
                if let Some(t) = self.tab_stops.get_mut(self.cursor.col as usize) {
                    *t = false;
                }
            }
            3 => {
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
        self.pending_bell = Some(BellEvent::Both);
    }

    fn reset(&mut self) {
        // Switch back to primary screen if on alternate
        if self.modes.alternate_screen {
            self.switch_to_primary();
        }

        self.cursor = Cursor::new();
        self.grid.clear();
        self.scroll_top = 0;
        self.scroll_bottom = self.grid.lines().saturating_sub(1);
        self.title.clear();
        self.icon_name.clear();
        self.modes = TerminalModes {
            cursor_visible: true,
            auto_wrap: true,
            ..Default::default()
        };
        self.cell_hyperlinks.clear();
        self.current_hyperlink = None;
        self.viewport_offset = 0;

        // Reset tab stops
        self.tab_stops.fill(false);
        for i in (0..self.cols as usize).step_by(8) {
            if i < self.tab_stops.len() {
                self.tab_stops[i] = true;
            }
        }
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

        // Move cursor to home if origin mode is set
        if self.modes.origin_mode {
            self.cursor.col = 0;
            self.cursor.line = self.scroll_top;
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
        self.modes.cursor_visible = visible;
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
            3 => {                                           // DECCOLM - 132/80 column mode
                // Would need to resize terminal - typically ignored
            }
            4 => {},                                         // DECSCLM - smooth scroll (ignored)
            5 => {},                                         // DECSCNM - reverse video (TODO)
            6 => {                                           // DECOM - origin mode
                self.modes.origin_mode = enable;
                // Move cursor to origin
                self.cursor.col = 0;
                self.cursor.line = if enable { self.scroll_top } else { 0 };
            }
            7 => self.modes.auto_wrap = enable,             // DECAWM
            12 => {},                                        // Cursor blink (ignored for now)
            20 => self.modes.linefeed_mode = enable,        // LNM
            25 => {                                          // DECTCEM
                self.cursor.visible = enable;
                self.modes.cursor_visible = enable;
            }
            47 => {                                          // Alternate screen (old)
                if enable {
                    self.switch_to_alternate();
                } else {
                    self.switch_to_primary();
                }
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
            1004 => self.modes.focus_reporting = enable,    // Focus events
            1005 => {},                                      // UTF-8 mouse mode (deprecated)
            1006 => {                                        // SGR mouse mode
                if enable {
                    self.modes.mouse_tracking = MouseMode::Sgr;
                }
            }
            1015 => {},                                      // URXVT mouse mode (deprecated)
            1047 => {                                        // Alternate screen (secondary)
                if enable {
                    self.switch_to_alternate();
                } else {
                    self.switch_to_primary();
                }
            }
            1048 => {                                        // Save/restore cursor
                if enable {
                    self.save_cursor();
                } else {
                    self.restore_cursor();
                }
            }
            1049 => {                                        // Alternate screen + cursor save
                if enable {
                    self.save_cursor();
                    self.switch_to_alternate();
                    self.grid.clear();
                } else {
                    self.switch_to_primary();
                    self.restore_cursor();
                }
            }
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
        // Handled at application level
    }

    fn decrqss(&mut self, query: &str) -> Option<Vec<u8>> {
        let response: String = match query {
            "m" => {
                let mut params: Vec<String> = Vec::new();

                if self.cursor.flags.contains(CellFlags::BOLD) {
                    params.push("1".to_string());
                }
                if self.cursor.flags.contains(CellFlags::DIM) {
                    params.push("2".to_string());
                }
                if self.cursor.flags.contains(CellFlags::ITALIC) {
                    params.push("3".to_string());
                }
                if self.cursor.flags.contains(CellFlags::UNDERLINE) {
                    params.push("4".to_string());
                }
                if self.cursor.flags.contains(CellFlags::BLINK) {
                    params.push("5".to_string());
                }
                if self.cursor.flags.contains(CellFlags::INVERSE) {
                    params.push("7".to_string());
                }
                if self.cursor.flags.contains(CellFlags::HIDDEN) {
                    params.push("8".to_string());
                }
                if self.cursor.flags.contains(CellFlags::STRIKETHROUGH) {
                    params.push("9".to_string());
                }

                let fg = &self.cursor.fg;
                params.push(format!("38;2;{};{};{}", fg.r, fg.g, fg.b));

                let bg = &self.cursor.bg;
                params.push(format!("48;2;{};{};{}", bg.r, bg.g, bg.b));

                if params.is_empty() {
                    "0m".to_string()
                } else {
                    format!("{}m", params.join(";"))
                }
            }
            "r" => {
                format!("{};{}r", self.scroll_top + 1, self.scroll_bottom + 1)
            }
            " q" => {
                "2 q".to_string()
            }
            "\"p" => {
                "64;1\"p".to_string()
            }
            "\"q" => {
                "0\"q".to_string()
            }
            _ => {
                return Some(b"\x1bP0$r\x1b\\".to_vec());
            }
        };

        Some(format!("\x1bP1$r{}\x1b\\", response).into_bytes())
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
        assert_eq!(term.grid.cell(0, 0).c(), 'A');
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
        assert_eq!(term.grid.cell(0, 0).c(), 'H');
        assert_eq!(term.grid.cell(4, 0).c(), 'o');
        assert_eq!(term.cursor.col, 5);
    }

    #[test]
    fn terminal_process_csi_cursor() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"\x1b[10;20H");
        assert_eq!(term.cursor.line, 9);
        assert_eq!(term.cursor.col, 19);
    }

    #[test]
    fn terminal_process_csi_erase() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"ABCDE");
        term.process(b"\x1b[H\x1b[2J");
        assert_eq!(term.grid.cell(0, 0).c(), ' ');
    }

    #[test]
    fn terminal_process_sgr() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"\x1b[1;31mRed");
        assert!(term.cursor.flags.contains(CellFlags::BOLD));
        assert_eq!(term.grid.cell(0, 0).c(), 'R');
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
        term.cursor.line = 4;
        term.linefeed();
        assert_eq!(term.cursor.line, 4);
        assert_eq!(term.grid.scrollback_len(), 1);
    }

    #[test]
    fn terminal_line_wrap() {
        let mut term = Terminal::new(5, 2, 1000);
        term.process(b"ABCDE");
        assert_eq!(term.cursor.col, 5);
        term.process(b"F");
        assert_eq!(term.cursor.line, 1);
        assert_eq!(term.cursor.col, 1);
    }

    #[test]
    fn terminal_alternate_screen() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"Primary");
        assert!(!term.is_alternate_screen());

        // Switch to alternate
        term.process(b"\x1b[?1049h");
        assert!(term.is_alternate_screen());
        assert_eq!(term.grid.cell(0, 0).c(), ' '); // Alternate is clear

        term.process(b"Alternate");

        // Switch back
        term.process(b"\x1b[?1049l");
        assert!(!term.is_alternate_screen());
        assert_eq!(term.grid.cell(0, 0).c(), 'P'); // Primary content restored
    }

    #[test]
    fn terminal_viewport_scroll() {
        let mut term = Terminal::new(80, 5, 100);
        
        // Generate some scrollback
        for i in 0..20 {
            term.process(format!("Line {}\n", i).as_bytes());
        }

        assert_eq!(term.viewport_offset, 0);
        assert!(term.grid.scrollback_len() > 0);

        term.scroll_viewport_up(5);
        assert_eq!(term.viewport_offset, 5);

        term.scroll_viewport_down(3);
        assert_eq!(term.viewport_offset, 2);

        term.reset_viewport();
        assert_eq!(term.viewport_offset, 0);
    }

    #[test]
    fn terminal_bell() {
        let mut term = Terminal::new(80, 24, 1000);
        assert!(term.take_pending_bell().is_none());

        term.bell();
        assert!(term.has_pending_bell());

        let bell = term.take_pending_bell();
        assert!(matches!(bell, Some(BellEvent::Both)));
        assert!(!term.has_pending_bell());
    }

    #[test]
    fn terminal_hyperlink() {
        let mut term = Terminal::new(80, 24, 1000);
        
        // Set hyperlink
        term.set_hyperlink(Some("test"), Some("https://example.com"));
        assert!(term.current_hyperlink().is_some());

        // Write some text with hyperlink
        term.process(b"Click here");

        // Check hyperlink was stored for cells
        assert!(term.get_cell_hyperlink(0, 0).is_some());

        // Clear hyperlink
        term.set_hyperlink(None, None);
        assert!(term.current_hyperlink().is_none());
    }

    #[test]
    fn terminal_origin_mode() {
        let mut term = Terminal::new(80, 24, 1000);
        
        // Set scroll region
        term.set_scroll_region(5, 15);
        
        // Enable origin mode
        term.set_mode(6, true);
        assert!(term.modes.origin_mode);
        assert_eq!(term.cursor.line, 4); // scroll_top (0-indexed)

        // goto should be relative to scroll region
        term.goto(0, 0);
        assert_eq!(term.cursor.line, 4);
    }

    #[test]
    fn terminal_application_keypad() {
        let mut term = Terminal::new(80, 24, 1000);
        assert!(!term.modes.application_keypad);

        // ESC = enables application keypad
        term.process(b"\x1b=");
        assert!(term.modes.application_keypad);

        // ESC > disables it
        term.process(b"\x1b>");
        assert!(!term.modes.application_keypad);
    }
}
