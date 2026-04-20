//! Keyboard event handling
//!
//! Converts key events to ANSI escape sequences for the PTY.

use crate::render::window::{KeyCode, Modifiers};

/// Keyboard handler converts key events to terminal escape sequences
pub struct KeyboardHandler {
    /// Application cursor key mode (DECCKM)
    pub application_cursor: bool,
    /// Application keypad mode (DECPAM/DECPNM)
    pub application_keypad: bool,
    /// Bracketed paste mode
    pub bracketed_paste: bool,
}

impl KeyboardHandler {
    /// Sync modes from terminal state
    pub fn sync_modes(&mut self, modes: &crate::term::terminal::TerminalModes) {
        self.application_cursor = modes.application_cursor;
        self.application_keypad = modes.application_keypad;
        self.bracketed_paste = modes.bracketed_paste;
    }

    /// Get bracketed paste start sequence
    pub fn bracketed_paste_start(&self) -> Option<&'static [u8]> {
        if self.bracketed_paste {
            Some(b"\x1b[200~")
        } else {
            None
        }
    }

    /// Get bracketed paste end sequence
    pub fn bracketed_paste_end(&self) -> Option<&'static [u8]> {
        if self.bracketed_paste {
            Some(b"\x1b[201~")
        } else {
            None
        }
    }
}

impl KeyboardHandler {
    pub fn new() -> Self {
        Self {
            application_cursor: false,
            application_keypad: false,
            bracketed_paste: false,
        }
    }

    /// Convert a character input to bytes for the PTY
    pub fn char_to_bytes(&self, c: char, modifiers: &Modifiers) -> Vec<u8> {
        // Handle Ctrl+key combinations
        if modifiers.ctrl {
            let code = match c.to_ascii_lowercase() {
                'a'..='z' => Some((c.to_ascii_lowercase() as u8) - b'a' + 1),
                ' ' => Some(0),   // Ctrl+Space = NUL
                '[' => Some(27),  // Ctrl+[ = ESC
                '\\' => Some(28), // Ctrl+\ = FS
                ']' => Some(29),  // Ctrl+] = GS
                '^' => Some(30),  // Ctrl+^ = RS
                '_' => Some(31),  // Ctrl+_ = US
                _ => None,
            };
            if let Some(byte) = code {
                return vec![byte];
            }
        }

        // Alt+key sends ESC prefix
        if modifiers.alt {
            let mut bytes = vec![0x1b];
            let mut char_buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut char_buf);
            bytes.extend_from_slice(encoded.as_bytes());
            return bytes;
        }

        // Regular character
        let mut buf = [0u8; 4];
        let encoded = c.encode_utf8(&mut buf);
        encoded.as_bytes().to_vec()
    }

    /// Convert a key event to bytes for the PTY
    pub fn key_to_bytes(&self, key: KeyCode, modifiers: &Modifiers) -> Option<Vec<u8>> {
        // Cursor keys depend on application mode
        let cursor_prefix = if self.application_cursor { b'O' } else { b'[' };

        let bytes = match key {
            // Cursor keys
            KeyCode::Up => vec![0x1b, cursor_prefix, b'A'],
            KeyCode::Down => vec![0x1b, cursor_prefix, b'B'],
            KeyCode::Right => vec![0x1b, cursor_prefix, b'C'],
            KeyCode::Left => vec![0x1b, cursor_prefix, b'D'],

            // Navigation
            KeyCode::Home => vec![0x1b, b'[', b'H'],
            KeyCode::End => vec![0x1b, b'[', b'F'],
            KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
            KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
            KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],
            KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],

            // Editing
            KeyCode::Backspace => vec![0x7f], // DEL
            KeyCode::Tab => {
                if modifiers.shift {
                    vec![0x1b, b'[', b'Z'] // Shift+Tab = backtab
                } else {
                    vec![0x09]
                }
            }
            KeyCode::Enter => vec![0x0d], // CR
            KeyCode::Escape => vec![0x1b],
            KeyCode::Space => vec![0x20],

            // Function keys
            KeyCode::F1 => vec![0x1b, b'O', b'P'],
            KeyCode::F2 => vec![0x1b, b'O', b'Q'],
            KeyCode::F3 => vec![0x1b, b'O', b'R'],
            KeyCode::F4 => vec![0x1b, b'O', b'S'],
            KeyCode::F5 => vec![0x1b, b'[', b'1', b'5', b'~'],
            KeyCode::F6 => vec![0x1b, b'[', b'1', b'7', b'~'],
            KeyCode::F7 => vec![0x1b, b'[', b'1', b'8', b'~'],
            KeyCode::F8 => vec![0x1b, b'[', b'1', b'9', b'~'],
            KeyCode::F9 => vec![0x1b, b'[', b'2', b'0', b'~'],
            KeyCode::F10 => vec![0x1b, b'[', b'2', b'1', b'~'],
            KeyCode::F11 => vec![0x1b, b'[', b'2', b'3', b'~'],
            KeyCode::F12 => vec![0x1b, b'[', b'2', b'4', b'~'],

            // Character key - handle via char_to_bytes
            KeyCode::Character(c) => return Some(self.char_to_bytes(c, modifiers)),
        };

        // Add modifiers for CSI sequences (but not for Tab which handles shift specially)
        let should_add_mods = (modifiers.ctrl || modifiers.alt || modifiers.shift)
            && bytes.len() > 2
            && bytes[1] == b'['
            && !matches!(key, KeyCode::Tab);

        if should_add_mods {
            Some(add_modifiers(&bytes, modifiers))
        } else {
            Some(bytes)
        }
    }
}

impl Default for KeyboardHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Add modifier encoding to CSI sequences
fn add_modifiers(bytes: &[u8], modifiers: &Modifiers) -> Vec<u8> {
    // Modifier encoding: 1 + (shift?1:0) + (alt?2:0) + (ctrl?4:0) + (meta?8:0)
    let mut modifier = 1;
    if modifiers.shift { modifier += 1; }
    if modifiers.alt { modifier += 2; }
    if modifiers.ctrl { modifier += 4; }
    if modifiers.meta { modifier += 8; }

    if modifier == 1 {
        return bytes.to_vec();
    }

    // Convert ESC [ X to ESC [ 1 ; mod X
    // or ESC [ N ~ to ESC [ N ; mod ~
    let mut result = Vec::new();
    result.push(bytes[0]); // ESC
    result.push(bytes[1]); // [

    if bytes.len() >= 3 && bytes[bytes.len() - 1] == b'~' {
        // Tilde-terminated sequence
        result.extend_from_slice(&bytes[2..bytes.len() - 1]);
        result.push(b';');
        result.extend_from_slice(modifier.to_string().as_bytes());
        result.push(b'~');
    } else if bytes.len() == 3 {
        // Simple sequence like ESC [ A
        result.push(b'1');
        result.push(b';');
        result.extend_from_slice(modifier.to_string().as_bytes());
        result.push(bytes[2]);
    } else {
        // Unknown format, return unchanged
        return bytes.to_vec();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_mods() -> Modifiers {
        Modifiers::default()
    }

    fn ctrl() -> Modifiers {
        Modifiers { ctrl: true, ..Default::default() }
    }

    fn alt() -> Modifiers {
        Modifiers { alt: true, ..Default::default() }
    }

    fn shift() -> Modifiers {
        Modifiers { shift: true, ..Default::default() }
    }

    #[test]
    fn char_simple() {
        let handler = KeyboardHandler::new();
        assert_eq!(handler.char_to_bytes('a', &no_mods()), vec![b'a']);
        assert_eq!(handler.char_to_bytes('A', &no_mods()), vec![b'A']);
    }

    #[test]
    fn char_utf8() {
        let handler = KeyboardHandler::new();
        let bytes = handler.char_to_bytes('ñ', &no_mods());
        assert_eq!(bytes, "ñ".as_bytes());
    }

    #[test]
    fn ctrl_c() {
        let handler = KeyboardHandler::new();
        assert_eq!(handler.char_to_bytes('c', &ctrl()), vec![3]); // ETX
    }

    #[test]
    fn ctrl_a() {
        let handler = KeyboardHandler::new();
        assert_eq!(handler.char_to_bytes('a', &ctrl()), vec![1]); // SOH
    }

    #[test]
    fn alt_key() {
        let handler = KeyboardHandler::new();
        let bytes = handler.char_to_bytes('x', &alt());
        assert_eq!(bytes, vec![0x1b, b'x']); // ESC x
    }

    #[test]
    fn key_arrow_up() {
        let handler = KeyboardHandler::new();
        let bytes = handler.key_to_bytes(KeyCode::Up, &no_mods()).unwrap();
        assert_eq!(bytes, vec![0x1b, b'[', b'A']);
    }

    #[test]
    fn key_arrow_application_mode() {
        let mut handler = KeyboardHandler::new();
        handler.application_cursor = true;
        let bytes = handler.key_to_bytes(KeyCode::Up, &no_mods()).unwrap();
        assert_eq!(bytes, vec![0x1b, b'O', b'A']); // O instead of [
    }

    #[test]
    fn key_enter() {
        let handler = KeyboardHandler::new();
        let bytes = handler.key_to_bytes(KeyCode::Enter, &no_mods()).unwrap();
        assert_eq!(bytes, vec![0x0d]); // CR
    }

    #[test]
    fn key_backspace() {
        let handler = KeyboardHandler::new();
        let bytes = handler.key_to_bytes(KeyCode::Backspace, &no_mods()).unwrap();
        assert_eq!(bytes, vec![0x7f]); // DEL
    }

    #[test]
    fn key_f1() {
        let handler = KeyboardHandler::new();
        let bytes = handler.key_to_bytes(KeyCode::F1, &no_mods()).unwrap();
        assert_eq!(bytes, vec![0x1b, b'O', b'P']);
    }

    #[test]
    fn key_delete() {
        let handler = KeyboardHandler::new();
        let bytes = handler.key_to_bytes(KeyCode::Delete, &no_mods()).unwrap();
        assert_eq!(bytes, vec![0x1b, b'[', b'3', b'~']);
    }

    #[test]
    fn key_shift_tab() {
        let handler = KeyboardHandler::new();
        let bytes = handler.key_to_bytes(KeyCode::Tab, &shift()).unwrap();
        assert_eq!(bytes, vec![0x1b, b'[', b'Z']); // Backtab
    }

    #[test]
    fn key_ctrl_arrow() {
        let handler = KeyboardHandler::new();
        let bytes = handler.key_to_bytes(KeyCode::Up, &ctrl()).unwrap();
        // ESC [ 1 ; 5 A (5 = 1 + ctrl)
        assert_eq!(bytes, vec![0x1b, b'[', b'1', b';', b'5', b'A']);
    }
}
