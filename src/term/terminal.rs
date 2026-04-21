//! Terminal state combining grid, cursor, and parser

use unicode_segmentation::UnicodeSegmentation;
use crate::ansi::{Attr, Handler, Parser, Action, csi_dispatch, osc_dispatch};
use crate::render::sixel::SixelDecoder;
use super::cell::{CellFlags, Color, ColorPalette, SharedGraphemeStorage, new_grapheme_storage};
use super::cursor::{Cursor, CursorShape, SavedCursor};
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
    /// Character with optional grapheme key (0 = single char, >0 = multi-codepoint)
    Char(char, u32),
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
    /// Alternate screen buffer active
    pub alternate_screen: bool,
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

/// Character set for G0-G3 designation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Charset {
    /// US ASCII (B)
    #[default]
    Ascii,
    /// DEC Special Graphics / Line Drawing (0)
    DecSpecialGraphics,
    /// UK (A)
    Uk,
    /// DEC Supplemental (%)
    DecSupplemental,
}

/// Which charset slot is active
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CharsetSlot {
    #[default]
    G0,
    G1,
    G2,
    G3,
}

/// Convert byte to charset
fn charset_from_byte(byte: u8) -> Charset {
    match byte {
        b'B' => Charset::Ascii,
        b'0' => Charset::DecSpecialGraphics,
        b'A' => Charset::Uk,
        b'<' | b'%' => Charset::DecSupplemental,
        _ => Charset::Ascii,
    }
}

/// DEC Special Graphics character mapping (line drawing characters)
fn translate_dec_special(c: char) -> char {
    match c {
        'j' => '┘', // Lower right corner
        'k' => '┐', // Upper right corner
        'l' => '┌', // Upper left corner
        'm' => '└', // Lower left corner
        'n' => '┼', // Crossing lines
        'q' => '─', // Horizontal line
        't' => '├', // Left tee
        'u' => '┤', // Right tee
        'v' => '┴', // Bottom tee
        'w' => '┬', // Top tee
        'x' => '│', // Vertical line
        'a' => '▒', // Checkerboard
        'f' => '°', // Degree symbol
        'g' => '±', // Plus/minus
        'h' => '░', // Board of squares
        'i' => '␋', // Lantern symbol
        'o' => '⎺', // Scan line 1
        'p' => '⎻', // Scan line 3
        'r' => '⎼', // Scan line 7
        's' => '⎽', // Scan line 9
        '`' => '◆', // Diamond
        '~' => '·', // Middle dot
        'y' => '≤', // Less than or equal
        'z' => '≥', // Greater than or equal
        '{' => 'π', // Pi
        '|' => '≠', // Not equal
        '}' => '£', // Pound sign
        _ => c,
    }
}

/// Active hyperlink
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hyperlink {
    pub id: Option<String>,
    pub url: String,
}

/// Stored hyperlink for reference by cells
#[derive(Debug, Clone)]
pub struct StoredHyperlink {
    pub id: u32,
    pub url: String,
    pub id_str: Option<String>,
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

/// Shell integration prompt region markers (OSC 133)
#[derive(Debug, Clone, Default)]
pub struct PromptRegion {
    /// Start position of prompt
    pub prompt_start: Option<(u16, u16)>,
    /// Start position of command input
    pub command_start: Option<(u16, u16)>,
    /// Start position of command output
    pub output_start: Option<(u16, u16)>,
    /// Exit code of last command
    pub last_exit_code: Option<i32>,
}

/// Clipboard operation request (for OSC 52)
#[derive(Debug, Clone)]
pub struct ClipboardRequest {
    /// Selection target (c=clipboard, p=primary, etc.)
    pub selection: char,
    /// Data to set (None = query, Some("") = clear, Some(data) = set)
    pub data: Option<String>,
}

/// Clipboard callback for OSC 52
pub type ClipboardCallback = Box<dyn Fn(ClipboardRequest) + Send + Sync>;

/// Search match position
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// Column position
    pub col: u16,
    /// Row position (in scrollback, negative values are history)
    pub row: i32,
    /// Length of match in cells
    pub len: u16,
}

/// Scrollback search state
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    /// Current search query
    pub query: String,
    /// All matches found
    pub matches: Vec<SearchMatch>,
    /// Index of current/active match
    pub current_match: usize,
    /// Whether search is active
    pub active: bool,
}

/// Complete terminal state
pub struct Terminal {
    /// Primary screen grid
    grid: Grid,
    /// Alternate screen grid (for vim, less, etc.)
    alt_grid: Grid,
    /// Current cursor
    cursor: Cursor,
    /// Saved cursor for primary screen
    saved_cursor: Option<SavedCursor>,
    /// Saved cursor for alternate screen
    alt_saved_cursor: Option<SavedCursor>,
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
    /// Current hyperlink ID for cell tagging
    current_hyperlink_id: u32,
    /// Next hyperlink ID to assign
    next_hyperlink_id: u32,
    /// Stored hyperlinks by ID
    hyperlinks: std::collections::HashMap<u32, StoredHyperlink>,
    /// Working directory (OSC 7)
    working_directory: Option<String>,
    /// Pending bell event
    pending_bell: Option<BellEvent>,
    /// Shell integration prompt regions
    prompt_region: PromptRegion,
    /// Clipboard callback for OSC 52
    clipboard_callback: Option<ClipboardCallback>,
    /// Cell height for sixel calculations (from font metrics)
    cell_height_pixels: u32,
    /// Search state for scrollback search
    search: SearchState,
    /// G0 character set
    charset_g0: Charset,
    /// G1 character set
    charset_g1: Charset,
    /// G2 character set
    charset_g2: Charset,
    /// G3 character set
    charset_g3: Charset,
    /// Active character set (G0 or G1)
    active_charset: CharsetSlot,
    /// PTY write callback for terminal responses
    pty_writer: Option<Box<dyn Fn(&[u8]) + Send + Sync>>,
    /// Color palette for ANSI colors (configurable)
    color_palette: ColorPalette,
    /// Grapheme cluster storage for multi-codepoint characters
    grapheme_storage: SharedGraphemeStorage,
    /// Scrollback viewport offset (0 = live view, >0 = viewing history)
    viewport_offset: usize,
    /// Terminal dimensions
    cols: u16,
    rows: u16,
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
            alt_grid: Grid::new(cols, rows, 0), // Alternate screen has no scrollback
            cursor: Cursor::new(),
            saved_cursor: None,
            alt_saved_cursor: None,
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
            current_hyperlink_id: 0,
            next_hyperlink_id: 1,
            hyperlinks: std::collections::HashMap::new(),
            working_directory: None,
            pending_bell: None,
            prompt_region: PromptRegion::default(),
            clipboard_callback: None,
            cell_height_pixels: 16, // Default, should be set from font metrics
            search: SearchState::default(),
            charset_g0: Charset::Ascii,
            charset_g1: Charset::DecSpecialGraphics,
            charset_g2: Charset::Ascii,
            charset_g3: Charset::Ascii,
            active_charset: CharsetSlot::G0,
            pty_writer: None,
            color_palette: ColorPalette::default(),
            grapheme_storage: new_grapheme_storage(),
            viewport_offset: 0,
            cols,
            rows,
        }
    }

    /// Set the PTY writer callback for terminal responses
    pub fn set_pty_writer<F>(&mut self, writer: F)
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        self.pty_writer = Some(Box::new(writer));
    }

    /// Write data to PTY (for terminal responses)
    fn write_pty(&self, data: &[u8]) {
        if let Some(writer) = &self.pty_writer {
            writer(data);
        }
    }

    /// Set the color palette
    pub fn set_color_palette(&mut self, palette: ColorPalette) {
        self.color_palette = palette;
    }

    /// Get the color palette
    pub fn color_palette(&self) -> &ColorPalette {
        &self.color_palette
    }

    /// Get the grapheme storage
    pub fn grapheme_storage(&self) -> &SharedGraphemeStorage {
        &self.grapheme_storage
    }

    /// Get grapheme string by key
    pub fn get_grapheme(&self, key: u32) -> Option<String> {
        if key == 0 {
            return None;
        }
        self.grapheme_storage.read().ok().and_then(|storage| {
            storage.get(key).map(|s| s.to_string())
        })
    }

    /// Get search state
    pub fn search(&self) -> &SearchState {
        &self.search
    }

    /// Start an incremental search
    pub fn search_start(&mut self, query: &str) {
        self.search.query = query.to_string();
        self.search.active = true;
        self.search_update();
    }

    /// Update search with new query (incremental)
    pub fn search_update(&mut self) {
        self.search.matches.clear();
        self.search.current_match = 0;

        if self.search.query.is_empty() {
            return;
        }

        let query = self.search.query.to_lowercase();

        // Search in visible grid
        for row in 0..self.grid.lines() {
            let line_text: String = (0..self.grid.cols())
                .map(|col| self.grid.cell(col, row).c)
                .collect();
            let line_lower = line_text.to_lowercase();

            let mut start = 0;
            while let Some(pos) = line_lower[start..].find(&query) {
                let col = (start + pos) as u16;
                self.search.matches.push(SearchMatch {
                    col,
                    row: row as i32,
                    len: query.len() as u16,
                });
                start += pos + 1;
            }
        }

        // Search in scrollback (negative row indices)
        for offset in 0..self.grid.scrollback_len() {
            if let Some(row_data) = self.grid.scrollback_row(offset) {
                let line_text: String = row_data.cells.iter().map(|c| c.c).collect();
                let line_lower = line_text.to_lowercase();

                let mut start = 0;
                while let Some(pos) = line_lower[start..].find(&query) {
                    let col = (start + pos) as u16;
                    self.search.matches.push(SearchMatch {
                        col,
                        row: -(offset as i32 + 1),
                        len: query.len() as u16,
                    });
                    start += pos + 1;
                }
            }
        }
    }

    /// Go to next search match
    pub fn search_next(&mut self) {
        if !self.search.matches.is_empty() {
            self.search.current_match = (self.search.current_match + 1) % self.search.matches.len();
        }
    }

    /// Go to previous search match
    pub fn search_prev(&mut self) {
        if !self.search.matches.is_empty() {
            if self.search.current_match == 0 {
                self.search.current_match = self.search.matches.len() - 1;
            } else {
                self.search.current_match -= 1;
            }
        }
    }

    /// Cancel search
    pub fn search_cancel(&mut self) {
        self.search.active = false;
        self.search.query.clear();
        self.search.matches.clear();
        self.search.current_match = 0;
    }

    /// Get current search match (if any)
    pub fn current_search_match(&self) -> Option<&SearchMatch> {
        if self.search.active && !self.search.matches.is_empty() {
            self.search.matches.get(self.search.current_match)
        } else {
            None
        }
    }

    /// Set cell height in pixels (from font metrics) for sixel rendering
    pub fn set_cell_height(&mut self, height: u32) {
        self.cell_height_pixels = height;
    }

    /// Set clipboard callback for OSC 52
    pub fn set_clipboard_callback(&mut self, callback: ClipboardCallback) {
        self.clipboard_callback = Some(callback);
    }

    /// Get hyperlink by ID
    pub fn hyperlink(&self, id: u32) -> Option<&StoredHyperlink> {
        self.hyperlinks.get(&id)
    }

    /// Get hyperlink at cell position
    pub fn get_cell_hyperlink(&self, col: u16, row: u16) -> Option<&StoredHyperlink> {
        if col < self.grid.cols() && row < self.grid.lines() {
            let cell = self.grid.cell(col, row);
            if cell.hyperlink_id != 0 {
                return self.hyperlinks.get(&cell.hyperlink_id);
            }
        }
        None
    }

    /// Get prompt region for shell integration
    pub fn prompt_region(&self) -> &PromptRegion {
        &self.prompt_region
    }

    /// Update cursor blink state
    pub fn update_cursor_blink(&mut self) -> bool {
        self.cursor.update_blink()
    }

    /// Reset cursor blink (call on keypress)
    pub fn reset_cursor_blink(&mut self) {
        self.cursor.reset_blink();
    }

    /// Take pending bell event
    pub fn take_pending_bell(&mut self) -> Option<BellEvent> {
        self.pending_bell.take()
    }

    /// Check if there's a pending bell
    pub fn has_pending_bell(&self) -> bool {
        self.pending_bell.is_some()
    }

    /// Switch to alternate screen buffer
    fn switch_to_alternate_screen(&mut self) {
        if self.modes.alternate_screen {
            return; // Already on alternate screen
        }
        self.modes.alternate_screen = true;
        // Save primary cursor
        self.saved_cursor = Some(self.cursor.save());
        // Switch grids (swap references)
        std::mem::swap(&mut self.grid, &mut self.alt_grid);
        // Clear alternate screen
        self.grid.clear();
        // Reset cursor position
        self.cursor.col = 0;
        self.cursor.line = 0;
        // Reset viewport
        self.viewport_offset = 0;
    }

    /// Switch back to primary screen buffer
    fn switch_to_primary_screen(&mut self) {
        if !self.modes.alternate_screen {
            return; // Already on primary screen
        }
        self.modes.alternate_screen = false;
        // Save alternate cursor
        self.alt_saved_cursor = Some(self.cursor.save());
        // Switch grids back
        std::mem::swap(&mut self.grid, &mut self.alt_grid);
        // Restore primary cursor
        if let Some(saved) = &self.saved_cursor {
            self.cursor.restore(saved);
        }
    }

    /// Check if using alternate screen
    pub fn is_alternate_screen(&self) -> bool {
        self.modes.alternate_screen
    }

    /// Set tab stop at current cursor position (HTS)
    pub fn set_tab_stop(&mut self) {
        if let Some(t) = self.tab_stops.get_mut(self.cursor.col as usize) {
            *t = true;
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
                        // Check if this is a multi-codepoint grapheme
                        let grapheme_key = if grapheme.chars().count() > 1 {
                            // Store the full grapheme and get a key
                            if let Ok(mut storage) = self.grapheme_storage.write() {
                                storage.store(grapheme)
                            } else {
                                0
                            }
                        } else {
                            0
                        };
                        chars_to_process.push(ProcessItem::Char(c, grapheme_key));
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
                ProcessItem::Char(c, grapheme_key) => {
                    if let Some(action) = self.parser.advance_char(c) {
                        self.execute(action);
                    } else if self.parser.is_ground() {
                        self.input_grapheme(c, grapheme_key);
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
            Action::EscDispatch { intermediates, final_byte } => {
                self.execute_esc(&intermediates, final_byte);
            }
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

    /// Execute an ESC sequence with intermediates
    fn execute_esc(&mut self, intermediates: &[u8], final_byte: u8) {
        match intermediates.first() {
            Some(b'(') => {
                // Designate G0 character set
                self.charset_g0 = charset_from_byte(final_byte);
            }
            Some(b')') => {
                // Designate G1 character set
                self.charset_g1 = charset_from_byte(final_byte);
            }
            Some(b'*') => {
                // Designate G2 character set (less common)
                self.charset_g2 = charset_from_byte(final_byte);
            }
            Some(b'+') => {
                // Designate G3 character set (less common)
                self.charset_g3 = charset_from_byte(final_byte);
            }
            Some(b'#') => {
                // DEC line attributes
                match final_byte {
                    b'8' => {
                        // DECALN - Screen Alignment Pattern (fill with E)
                        for row in 0..self.grid.lines() {
                            for col in 0..self.grid.cols() {
                                self.grid.cell_mut(col, row).c = 'E';
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Execute a control character or escape sequence final byte
    fn execute_control(&mut self, byte: u8) {
        match byte {
            // C0 control characters
            0x07 => self.bell(),           // BEL
            0x08 => self.backspace(),      // BS
            0x09 => self.tab(),            // HT
            0x0A => self.linefeed(),       // LF
            0x0B => self.linefeed(),       // VT (treated as LF)
            0x0C => self.linefeed(),       // FF (treated as LF)
            0x0D => self.carriage_return(), // CR
            0x0E => self.active_charset = CharsetSlot::G1, // SO (Shift Out) - use G1
            0x0F => self.active_charset = CharsetSlot::G0, // SI (Shift In) - use G0

            // ESC sequence final bytes (from escape state)
            b'7' => self.save_cursor(),    // DECSC - Save Cursor
            b'8' => self.restore_cursor(), // DECRC - Restore Cursor
            b'c' => self.reset(),          // RIS - Full Reset
            b'D' => self.linefeed(),       // IND - Index (linefeed)
            b'E' => {                      // NEL - Next Line
                self.carriage_return();
                self.linefeed();
            }
            b'H' => self.set_tab_stop(),   // HTS - Horizontal Tab Set
            b'M' => self.reverse_index(),  // RI - Reverse Index
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
    fn handle_dcs(&mut self, _params: &[u16], intermediates: &[u8], data: &[u8]) {
        if data.is_empty() {
            return;
        }

        // Check for DECRQSS (Request Selection or Setting Status)
        // Format: DCS $ q Pt ST
        if intermediates == b"$" && data.first() == Some(&b'q') {
            self.handle_decrqss(&data[1..]);
            return;
        }

        // Check for Kitty graphics protocol (APC G ... ST)
        if let Some(&first) = data.first() {
            if first == b'G' || (intermediates.first() == Some(&b'G')) {
                self.handle_kitty_graphics(&data[1..]);
                return;
            }
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

            // Advance cursor past the image (sixel spec says cursor moves down)
            let cell_height = self.cell_height_pixels.max(1);
            let rows_needed = (image.height + cell_height - 1) / cell_height;

            for _ in 0..rows_needed {
                self.linefeed();
            }
        }
    }

    /// Handle DECRQSS (Request Selection or Setting Status)
    fn handle_decrqss(&mut self, request: &[u8]) {
        let request_str = std::str::from_utf8(request).unwrap_or("");

        // Build response based on request
        let response = match request_str {
            // DECSCUSR - Cursor style
            " q" => {
                let style = match self.cursor.shape {
                    CursorShape::Block => if self.cursor.blink.enabled { 1 } else { 2 },
                    CursorShape::Underline => if self.cursor.blink.enabled { 3 } else { 4 },
                    CursorShape::Beam => if self.cursor.blink.enabled { 5 } else { 6 },
                };
                format!("\x1bP1$r{} q\x1b\\", style)
            }
            // DECSTBM - Scroll region
            "r" => {
                format!("\x1bP1$r{};{}r\x1b\\", self.scroll_top + 1, self.scroll_bottom + 1)
            }
            // SGR - Graphics rendition
            "m" => {
                let mut sgr = Vec::new();
                if self.cursor.flags.contains(CellFlags::BOLD) { sgr.push("1"); }
                if self.cursor.flags.contains(CellFlags::DIM) { sgr.push("2"); }
                if self.cursor.flags.contains(CellFlags::ITALIC) { sgr.push("3"); }
                if self.cursor.flags.contains(CellFlags::UNDERLINE) { sgr.push("4"); }
                if self.cursor.flags.contains(CellFlags::BLINK) { sgr.push("5"); }
                if self.cursor.flags.contains(CellFlags::INVERSE) { sgr.push("7"); }
                if self.cursor.flags.contains(CellFlags::HIDDEN) { sgr.push("8"); }
                if self.cursor.flags.contains(CellFlags::STRIKETHROUGH) { sgr.push("9"); }
                let sgr_str = if sgr.is_empty() { "0".to_string() } else { sgr.join(";") };
                format!("\x1bP1$r{}m\x1b\\", sgr_str)
            }
            // DECSCL - Conformance level (report VT400)
            "\"p" => "\x1bP1$r64;1\"p\x1b\\".to_string(),
            // Unknown request - send invalid response
            _ => "\x1bP0$r\x1b\\".to_string(),
        };

        self.write_pty(response.as_bytes());
    }

    /// Handle Kitty graphics protocol
    fn handle_kitty_graphics(&mut self, data: &[u8]) {
        use crate::ansi::kitty::KittyDecoder;

        // Create decoder and parse
        let mut decoder = KittyDecoder::new();
        match decoder.parse(data) {
            Ok(Some(image)) => {
                // Place image as sixel placement for unified rendering
                let placement = SixelPlacement {
                    data: image.data,
                    width: image.width,
                    height: image.height,
                    col: self.cursor.col,
                    row: self.cursor.line,
                };
                self.sixel_images.push(placement);

                // Advance cursor
                let cell_height = self.cell_height_pixels.max(1);
                let rows_needed = (image.height + cell_height - 1) / cell_height;
                for _ in 0..rows_needed {
                    self.linefeed();
                }
            }
            Ok(None) => {
                // Multi-chunk transmission or other action that doesn't produce image
            }
            Err(e) => {
                log::debug!("Kitty graphics error: {}", e);
            }
        }
    }

    /// Get the grid
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
        self.alt_grid.resize(cols, rows);
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

    /// Set cursor shape (DECSCUSR)
    pub fn set_cursor_shape(&mut self, shape: CursorShape) {
        self.cursor.shape = shape;
    }

    /// Write a character at cursor position, advancing cursor
    fn write_char(&mut self, c: char) {
        self.write_char_with_grapheme(c, 0);
    }

    /// Write a character with an associated grapheme cluster key
    fn write_char_with_grapheme(&mut self, c: char, grapheme_key: u32) {
        let cols = self.grid.cols();

        // Translate character based on active charset
        let c = self.translate_char(c);

        // Handle wide characters
        let width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) as u16;

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

        // Write the character
        let cell = self.grid.cell_mut(self.cursor.col, self.cursor.line);
        cell.c = c;
        cell.grapheme_key = grapheme_key;
        cell.fg = self.cursor.fg;
        cell.bg = self.cursor.bg;
        cell.flags = self.cursor.flags;
        cell.hyperlink_id = self.current_hyperlink_id;

        if width > 1 {
            cell.flags |= CellFlags::WIDE;
            if self.cursor.col + 1 < cols {
                let spacer = self.grid.cell_mut(self.cursor.col + 1, self.cursor.line);
                spacer.c = ' ';
                spacer.grapheme_key = 0;
                spacer.flags = CellFlags::WIDE_SPACER;
                spacer.hyperlink_id = self.current_hyperlink_id;
            }
        }

        self.cursor.col += width;
    }

    /// Clear sixel images that are outside the visible screen or scrolled off
    fn clear_scrolled_sixel_images(&mut self) {
        let rows = self.grid.lines();
        self.sixel_images.retain(|img| {
            // Calculate row span of image
            let cell_height = self.cell_height_pixels.max(1);
            let img_rows = (img.height + cell_height - 1) / cell_height;
            let end_row = img.row as u32 + img_rows;
            // Keep if still visible
            end_row > 0 && (img.row as u32) < rows as u32
        });
    }

    /// Translate character based on active charset
    fn translate_char(&self, c: char) -> char {
        let charset = match self.active_charset {
            CharsetSlot::G0 => self.charset_g0,
            CharsetSlot::G1 => self.charset_g1,
            CharsetSlot::G2 => self.charset_g2,
            CharsetSlot::G3 => self.charset_g3,
        };

        match charset {
            Charset::DecSpecialGraphics => translate_dec_special(c),
            _ => c,
        }
    }

    /// Input a character with grapheme cluster support
    pub fn input_grapheme(&mut self, c: char, grapheme_key: u32) {
        self.write_char_with_grapheme(c, grapheme_key);
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
        // When DECOM (origin mode) is set, line is relative to scroll region
        let actual_line = if self.modes.origin_mode {
            (self.scroll_top + line).min(self.scroll_bottom)
        } else {
            line.min(self.grid.lines().saturating_sub(1))
        };
        self.cursor.line = actual_line;
        self.cursor.col = col.min(self.grid.cols().saturating_sub(1));
    }

    fn goto_line(&mut self, line: u16) {
        // When DECOM (origin mode) is set, line is relative to scroll region
        let actual_line = if self.modes.origin_mode {
            (self.scroll_top + line).min(self.scroll_bottom)
        } else {
            line.min(self.grid.lines().saturating_sub(1))
        };
        self.cursor.line = actual_line;
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
        }
    }

    fn delete_chars(&mut self, n: u16) {
        let cols = self.grid.cols();
        let line = self.cursor.line;
        let col = self.cursor.col;

        for c in col..cols.saturating_sub(n) {
            let src = self.grid.cell(c + n, line).clone();
            *self.grid.cell_mut(c, line) = src;
        }

        for c in cols.saturating_sub(n)..cols {
            self.grid.cell_mut(c, line).reset();
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
                }
                // Clear sixel images below cursor
                let cursor_line = self.cursor.line;
                self.sixel_images.retain(|img| img.row < cursor_line);
            }
            1 => {
                for line in 0..self.cursor.line {
                    self.grid.clear_line(line);
                }
                self.erase_in_line(1);
                // Clear sixel images above cursor
                let cursor_line = self.cursor.line;
                self.sixel_images.retain(|img| img.row > cursor_line);
            }
            2 => {
                self.grid.clear();
                // Clear all sixel images
                self.sixel_images.clear();
            }
            3 => {
                // Entire screen + scrollback
                self.grid.clear();
                // Clear all sixel images
                self.sixel_images.clear();
            }
            _ => {}
        }
        let _ = cols; // suppress unused warning
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
            Attr::ForegroundIndex(idx) => {
                // Use color palette for indexed colors
                self.cursor.fg = Color::from_256_with_palette(idx, &self.color_palette);
            }
            Attr::BackgroundIndex(idx) => {
                // Use color palette for indexed colors
                self.cursor.bg = Color::from_256_with_palette(idx, &self.color_palette);
            }
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
            self.switch_to_primary_screen();
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
        self.current_hyperlink = None;
        self.current_hyperlink_id = 0;
        self.viewport_offset = 0;

        // Reset tab stops
        self.tab_stops.fill(false);
        for i in (0..self.cols as usize).step_by(8) {
            if i < self.tab_stops.len() {
                self.tab_stops[i] = true;
            }
        }

        // Reset charsets
        self.charset_g0 = Charset::Ascii;
        self.charset_g1 = Charset::DecSpecialGraphics;
        self.charset_g2 = Charset::Ascii;
        self.charset_g3 = Charset::Ascii;
        self.active_charset = CharsetSlot::G0;
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
        // Use scroll region if set, otherwise scroll entire screen
        if self.scroll_top == 0 && self.scroll_bottom == self.grid.lines().saturating_sub(1) {
            // Full screen scroll - use regular scroll with scrollback
            for _ in 0..n {
                self.grid.scroll_up(1);
            }
        } else {
            // Region scroll
            self.grid.scroll_region_up(self.scroll_top, self.scroll_bottom, n as usize);
        }
        // Update sixel image positions and remove ones that scroll off
        for img in &mut self.sixel_images {
            img.row = img.row.saturating_sub(n);
        }
        self.clear_scrolled_sixel_images();
    }

    fn scroll_down(&mut self, n: u16) {
        // Use scroll region if set, otherwise scroll entire screen
        if self.scroll_top == 0 && self.scroll_bottom == self.grid.lines().saturating_sub(1) {
            // Full screen scroll - use regular scroll
            for _ in 0..n {
                self.grid.scroll_down(1);
            }
        } else {
            // Region scroll
            self.grid.scroll_region_down(self.scroll_top, self.scroll_bottom, n as usize);
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
            47 => {                                          // Alternate screen (no clear)
                if enable {
                    self.switch_to_alternate_screen();
                } else {
                    self.switch_to_primary_screen();
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
            1006 => {                                        // SGR mouse mode
                if enable {
                    self.modes.mouse_tracking = MouseMode::Sgr;
                }
            }
            1047 => {                                        // Alternate screen with clear
                if enable {
                    self.switch_to_alternate_screen();
                    self.grid.clear();
                } else {
                    self.switch_to_primary_screen();
                }
            }
            1049 => {                                        // Alternate screen + save cursor
                if enable {
                    self.save_cursor();
                    self.switch_to_alternate_screen();
                    self.grid.clear();
                } else {
                    self.switch_to_primary_screen();
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
                // Create or find existing hyperlink
                let hyperlink_id = self.next_hyperlink_id;
                self.next_hyperlink_id += 1;

                let stored = StoredHyperlink {
                    id: hyperlink_id,
                    url: url.to_string(),
                    id_str: id.map(|s| s.to_string()),
                };
                self.hyperlinks.insert(hyperlink_id, stored);
                self.current_hyperlink_id = hyperlink_id;

                self.current_hyperlink = Some(Hyperlink {
                    id: id.map(|s| s.to_string()),
                    url: url.to_string(),
                });
            }
            _ => {
                self.current_hyperlink = None;
                self.current_hyperlink_id = 0;
            }
        }
    }

    fn set_working_directory(&mut self, path: &str) {
        self.working_directory = Some(path.to_string());
    }

    fn clipboard(&mut self, selection: char, data: Option<&str>) {
        // Forward clipboard request to callback if set
        if let Some(ref callback) = self.clipboard_callback {
            let request = ClipboardRequest {
                selection,
                data: data.map(|s| s.to_string()),
            };
            callback(request);
        }
    }

    fn set_cursor_shape(&mut self, shape: u16) {
        self.cursor.set_shape_decscusr(shape);
    }

    fn primary_device_attributes(&mut self) {
        // Respond as VT220 with ANSI color, Sixel, and other capabilities
        let response = b"\x1b[?62;1;2;4;6;9;15;22c";
        self.write_pty(response);
    }

    fn secondary_device_attributes(&mut self) {
        // Respond with device ID and firmware version
        let response = b"\x1b[>1;100;0c";
        self.write_pty(response);
    }

    fn tertiary_device_attributes(&mut self) {
        // Respond with unit ID
        let response = b"\x1bP!|00000000\x1b\\";
        self.write_pty(response);
    }

    fn device_status_report(&mut self, mode: u16) {
        match mode {
            5 => {
                // Status report: terminal is OK
                let response = b"\x1b[0n";
                self.write_pty(response);
            }
            6 => {
                // Cursor position report
                let line = self.cursor.line + 1; // 1-indexed
                let col = self.cursor.col + 1;
                let response = format!("\x1b[{};{}R", line, col);
                self.write_pty(response.as_bytes());
            }
            _ => {}
        }
    }

    fn designate_g0(&mut self, charset: char) {
        self.charset_g0 = charset_from_byte(charset as u8);
    }

    fn designate_g1(&mut self, charset: char) {
        self.charset_g1 = charset_from_byte(charset as u8);
    }

    fn shift_in(&mut self) {
        self.active_charset = CharsetSlot::G0;
    }

    fn shift_out(&mut self) {
        self.active_charset = CharsetSlot::G1;
    }

    fn soft_reset(&mut self) {
        // DECSTR - Soft Terminal Reset
        // Reset modes but preserve scrollback and grid content
        self.cursor = Cursor::new();
        self.saved_cursor = None;
        self.modes = TerminalModes {
            cursor_visible: true,
            auto_wrap: true,
            ..Default::default()
        };
        self.scroll_top = 0;
        self.scroll_bottom = self.grid.lines().saturating_sub(1);
        self.charset_g0 = Charset::Ascii;
        self.charset_g1 = Charset::DecSpecialGraphics;
        self.charset_g2 = Charset::Ascii;
        self.charset_g3 = Charset::Ascii;
        self.active_charset = CharsetSlot::G0;
        self.current_hyperlink = None;
        self.current_hyperlink_id = 0;

        // Reset tab stops to default
        for (i, t) in self.tab_stops.iter_mut().enumerate() {
            *t = i % 8 == 0;
        }
    }

    fn request_terminal_parameters(&mut self, mode: u16) {
        // DECREQTPARM - very legacy, rarely used
        if mode <= 1 {
            let response = format!("\x1b[{};1;1;112;112;1;0x", mode + 2);
            self.write_pty(response.as_bytes());
        }
    }

    fn write_to_pty(&mut self, data: &[u8]) {
        self.write_pty(data);
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
        term.process(b"\x1b[10;20H");
        assert_eq!(term.cursor.line, 9);
        assert_eq!(term.cursor.col, 19);
    }

    #[test]
    fn terminal_process_csi_erase() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"ABCDE");
        term.process(b"\x1b[H\x1b[2J");
        assert_eq!(term.grid.cell(0, 0).c, ' ');
    }

    #[test]
    fn terminal_process_sgr() {
        let mut term = Terminal::new(80, 24, 1000);
        term.process(b"\x1b[1;31mRed");
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
        assert_eq!(term.grid.cell(0, 0).c, ' '); // Alternate is clear

        term.process(b"Alternate");

        // Switch back
        term.process(b"\x1b[?1049l");
        assert!(!term.is_alternate_screen());
        assert_eq!(term.grid.cell(0, 0).c, 'P'); // Primary content restored
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

        // Check hyperlink was stored
        assert!(term.hyperlinks.contains_key(&1));

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
