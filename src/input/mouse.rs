//! Mouse event handling and escape sequence generation
//!
//! Converts mouse events to terminal escape sequences based on tracking mode.

use crate::term::terminal::MouseMode;

/// Mouse button
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    /// Scroll up
    WheelUp,
    /// Scroll down
    WheelDown,
    /// Mouse button 4 (back)
    Button4,
    /// Mouse button 5 (forward)
    Button5,
}

impl MouseButton {
    /// Get the button code for X10/normal mode
    fn code(&self) -> u8 {
        match self {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::WheelUp => 64,
            MouseButton::WheelDown => 65,
            MouseButton::Button4 => 128,
            MouseButton::Button5 => 129,
        }
    }
}

/// Mouse event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventType {
    Press,
    Release,
    Move,
    Drag,
}

/// Mouse event with position and modifiers
#[derive(Debug, Clone)]
pub struct MouseEvent {
    pub button: MouseButton,
    pub event_type: MouseEventType,
    pub col: u16,
    pub row: u16,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl MouseEvent {
    /// Create a new mouse event
    pub fn new(
        button: MouseButton,
        event_type: MouseEventType,
        col: u16,
        row: u16,
    ) -> Self {
        Self {
            button,
            event_type,
            col,
            row,
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    /// Set modifier keys
    pub fn with_modifiers(mut self, shift: bool, ctrl: bool, alt: bool) -> Self {
        self.shift = shift;
        self.ctrl = ctrl;
        self.alt = alt;
        self
    }

    /// Get the button/modifier byte for encoding
    fn button_byte(&self) -> u8 {
        let mut byte = self.button.code();

        // Add modifier bits
        if self.shift {
            byte |= 4;
        }
        if self.alt {
            byte |= 8;
        }
        if self.ctrl {
            byte |= 16;
        }

        // For motion events, add 32
        if matches!(self.event_type, MouseEventType::Move | MouseEventType::Drag) {
            byte |= 32;
        }

        byte
    }
}

/// Mouse tracking handler
pub struct MouseHandler {
    /// Current mouse tracking mode
    mode: MouseMode,
    /// Whether a button is currently pressed (for drag tracking)
    button_pressed: bool,
    /// Last reported position (to avoid duplicate motion reports)
    last_position: Option<(u16, u16)>,
}

impl Default for MouseHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseHandler {
    /// Create a new mouse handler
    pub fn new() -> Self {
        Self {
            mode: MouseMode::None,
            button_pressed: false,
            last_position: None,
        }
    }

    /// Set the mouse tracking mode
    pub fn set_mode(&mut self, mode: MouseMode) {
        self.mode = mode;
        // Reset state when mode changes
        self.button_pressed = false;
        self.last_position = None;
    }

    /// Get the current mouse tracking mode
    pub fn mode(&self) -> MouseMode {
        self.mode
    }

    /// Process a mouse event and return escape sequence bytes (if any)
    pub fn process(&mut self, event: MouseEvent) -> Option<Vec<u8>> {
        if self.mode == MouseMode::None {
            return None;
        }

        match event.event_type {
            MouseEventType::Press => {
                self.button_pressed = true;
                self.last_position = Some((event.col, event.row));
                Some(self.encode(&event))
            }
            MouseEventType::Release => {
                self.button_pressed = false;

                match self.mode {
                    MouseMode::X10 => None, // X10 doesn't report releases
                    MouseMode::Normal | MouseMode::ButtonMotion | MouseMode::AnyMotion | MouseMode::Sgr => {
                        Some(self.encode(&event))
                    }
                    MouseMode::None => None,
                }
            }
            MouseEventType::Move | MouseEventType::Drag => {
                // Check if we should report motion
                let should_report = match self.mode {
                    MouseMode::AnyMotion => true,
                    MouseMode::ButtonMotion => self.button_pressed,
                    _ => false,
                };

                if !should_report {
                    return None;
                }

                // Check if position changed
                if let Some((last_col, last_row)) = self.last_position {
                    if last_col == event.col && last_row == event.row {
                        return None; // No change
                    }
                }

                self.last_position = Some((event.col, event.row));
                Some(self.encode(&event))
            }
        }
    }

    /// Encode a mouse event as escape sequence bytes
    fn encode(&self, event: &MouseEvent) -> Vec<u8> {
        // Coordinates are 1-based and offset by 32 for X10/normal encoding
        let col = event.col.saturating_add(1);
        let row = event.row.saturating_add(1);

        match self.mode {
            MouseMode::Sgr => {
                // SGR mode: ESC [ < button ; col ; row M/m
                let suffix = if event.event_type == MouseEventType::Release { 'm' } else { 'M' };
                format!("\x1b[<{};{};{}{}", event.button_byte(), col, row, suffix)
                    .into_bytes()
            }
            _ => {
                // X10/Normal mode: ESC [ M button col row
                // Coordinates capped at 223 (255 - 32) for backward compatibility
                let button = event.button_byte() + 32;
                let col_byte = (col.min(223) + 32) as u8;
                let row_byte = (row.min(223) + 32) as u8;

                // For release in normal mode, button code is 3
                let button = if event.event_type == MouseEventType::Release {
                    3 + 32
                } else {
                    button
                };

                vec![0x1b, b'[', b'M', button, col_byte, row_byte]
            }
        }
    }

    /// Check if mouse tracking is currently enabled
    pub fn is_enabled(&self) -> bool {
        self.mode != MouseMode::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_handler_new() {
        let handler = MouseHandler::new();
        assert_eq!(handler.mode(), MouseMode::None);
        assert!(!handler.is_enabled());
    }

    #[test]
    fn mouse_handler_set_mode() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::Normal);
        assert_eq!(handler.mode(), MouseMode::Normal);
        assert!(handler.is_enabled());
    }

    #[test]
    fn mouse_event_press_none_mode() {
        let mut handler = MouseHandler::new();
        let event = MouseEvent::new(MouseButton::Left, MouseEventType::Press, 10, 5);
        let result = handler.process(event);
        assert!(result.is_none());
    }

    #[test]
    fn mouse_event_press_normal_mode() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::Normal);
        
        let event = MouseEvent::new(MouseButton::Left, MouseEventType::Press, 10, 5);
        let result = handler.process(event);
        
        assert!(result.is_some());
        let bytes = result.unwrap();
        assert_eq!(bytes[0], 0x1b); // ESC
        assert_eq!(bytes[1], b'[');
        assert_eq!(bytes[2], b'M');
    }

    #[test]
    fn mouse_event_x10_no_release() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::X10);
        
        let press = MouseEvent::new(MouseButton::Left, MouseEventType::Press, 10, 5);
        assert!(handler.process(press).is_some());

        let release = MouseEvent::new(MouseButton::Left, MouseEventType::Release, 10, 5);
        assert!(handler.process(release).is_none()); // X10 doesn't report releases
    }

    #[test]
    fn mouse_event_sgr_format() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::Sgr);
        
        let event = MouseEvent::new(MouseButton::Left, MouseEventType::Press, 10, 5);
        let result = handler.process(event);
        
        assert!(result.is_some());
        let bytes = result.unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // SGR format: ESC [ < button ; col ; row M
        assert!(s.starts_with("\x1b[<"));
        assert!(s.ends_with('M'));
    }

    #[test]
    fn mouse_event_sgr_release() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::Sgr);
        
        let event = MouseEvent::new(MouseButton::Left, MouseEventType::Release, 10, 5);
        let result = handler.process(event);
        
        assert!(result.is_some());
        let bytes = result.unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // Release ends with 'm'
        assert!(s.ends_with('m'));
    }

    #[test]
    fn mouse_motion_any_mode() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::AnyMotion);
        
        let event = MouseEvent::new(MouseButton::Left, MouseEventType::Move, 10, 5);
        let result = handler.process(event);
        
        assert!(result.is_some());
    }

    #[test]
    fn mouse_motion_button_mode_no_press() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::ButtonMotion);
        
        let event = MouseEvent::new(MouseButton::Left, MouseEventType::Move, 10, 5);
        let result = handler.process(event);
        
        assert!(result.is_none()); // No button pressed
    }

    #[test]
    fn mouse_motion_button_mode_with_press() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::ButtonMotion);
        
        // Press first
        let press = MouseEvent::new(MouseButton::Left, MouseEventType::Press, 10, 5);
        handler.process(press);

        // Now motion should report
        let motion = MouseEvent::new(MouseButton::Left, MouseEventType::Drag, 11, 5);
        let result = handler.process(motion);
        
        assert!(result.is_some());
    }

    #[test]
    fn mouse_no_duplicate_position() {
        let mut handler = MouseHandler::new();
        handler.set_mode(MouseMode::AnyMotion);
        
        let event1 = MouseEvent::new(MouseButton::Left, MouseEventType::Move, 10, 5);
        assert!(handler.process(event1).is_some());

        // Same position - should not report
        let event2 = MouseEvent::new(MouseButton::Left, MouseEventType::Move, 10, 5);
        assert!(handler.process(event2).is_none());

        // Different position - should report
        let event3 = MouseEvent::new(MouseButton::Left, MouseEventType::Move, 11, 5);
        assert!(handler.process(event3).is_some());
    }

    #[test]
    fn mouse_button_codes() {
        assert_eq!(MouseButton::Left.code(), 0);
        assert_eq!(MouseButton::Middle.code(), 1);
        assert_eq!(MouseButton::Right.code(), 2);
        assert_eq!(MouseButton::WheelUp.code(), 64);
        assert_eq!(MouseButton::WheelDown.code(), 65);
    }

    #[test]
    fn mouse_modifiers() {
        let event = MouseEvent::new(MouseButton::Left, MouseEventType::Press, 0, 0)
            .with_modifiers(true, true, false);
        
        let byte = event.button_byte();
        // Shift = 4, Ctrl = 16
        assert_eq!(byte & 4, 4);  // Shift
        assert_eq!(byte & 16, 16); // Ctrl
        assert_eq!(byte & 8, 0);   // No Alt
    }
}
