//! Mouse event handling
//!
//! Handles mouse input for selection, scrolling, and terminal mouse modes.

use crate::term::selection::{SelectionManager, SelectionType};
use crate::term::MouseMode;
use crate::term::Grid;

/// Mouse button types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

impl MouseButton {
    /// Get the button code for mouse tracking
    pub fn code(&self) -> u8 {
        match self {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::WheelUp => 64,
            MouseButton::WheelDown => 65,
        }
    }

    /// Create from winit mouse button
    pub fn from_winit(button: winit::event::MouseButton) -> Option<Self> {
        Some(match button {
            winit::event::MouseButton::Left => MouseButton::Left,
            winit::event::MouseButton::Middle => MouseButton::Middle,
            winit::event::MouseButton::Right => MouseButton::Right,
            _ => return None,
        })
    }
}

/// Mouse event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEvent {
    Press(MouseButton),
    Release(MouseButton),
    Move,
    Drag(MouseButton),
}

impl MouseEvent {
    /// Get the event code modifier for mouse tracking
    pub fn code_modifier(&self) -> u8 {
        match self {
            MouseEvent::Press(_) => 0,
            MouseEvent::Release(_) => 0,
            MouseEvent::Move => 32,
            MouseEvent::Drag(_) => 32,
        }
    }
}

/// Mouse handler for terminal input
pub struct MouseHandler {
    /// Current mouse position in cell coordinates
    cell_pos: (u16, u16),
    /// Previous mouse position for drag detection
    prev_cell_pos: (u16, u16),
    /// Currently pressed button
    pressed_button: Option<MouseButton>,
    /// Whether we're currently dragging
    dragging: bool,
    /// Click count for multi-click detection (1=single, 2=double, 3=triple)
    click_count: u8,
    /// Cell dimensions for coordinate conversion
    cell_width: f32,
    cell_height: f32,
}

impl MouseHandler {
    /// Create a new mouse handler
    pub fn new() -> Self {
        Self {
            cell_pos: (0, 0),
            prev_cell_pos: (0, 0),
            pressed_button: None,
            dragging: false,
            click_count: 1,
            cell_width: 10.0,
            cell_height: 20.0,
        }
    }

    /// Set cell dimensions for coordinate conversion
    pub fn set_cell_size(&mut self, width: f32, height: f32) {
        self.cell_width = width;
        self.cell_height = height;
    }

    /// Convert pixel position to cell coordinates
    pub fn pixel_to_cell(&self, x: f64, y: f64) -> (u16, u16) {
        let col = (x / self.cell_width as f64).max(0.0) as u16;
        let row = (y / self.cell_height as f64).max(0.0) as u16;
        (col, row)
    }

    /// Update mouse position from pixel coordinates
    pub fn update_position(&mut self, x: f64, y: f64) {
        self.prev_cell_pos = self.cell_pos;
        self.cell_pos = self.pixel_to_cell(x, y);
    }

    /// Get current cell position
    pub fn cell_position(&self) -> (u16, u16) {
        self.cell_pos
    }

    /// Handle mouse button press
    pub fn press(&mut self, button: MouseButton, click_count: u8) {
        self.pressed_button = Some(button);
        self.click_count = click_count;
        self.dragging = false;
    }

    /// Handle mouse button release
    pub fn release(&mut self, _button: MouseButton) {
        self.pressed_button = None;
        self.dragging = false;
    }

    /// Handle mouse movement
    pub fn motion(&mut self) {
        if self.pressed_button.is_some() && self.cell_pos != self.prev_cell_pos {
            self.dragging = true;
        }
    }

    /// Check if we're dragging
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Get the pressed button if any
    pub fn pressed_button(&self) -> Option<MouseButton> {
        self.pressed_button
    }

    /// Get click count
    pub fn click_count(&self) -> u8 {
        self.click_count
    }

    /// Encode mouse event for terminal
    /// Returns the escape sequence to send to the terminal
    pub fn encode_event(
        &self,
        event: MouseEvent,
        mode: MouseMode,
        col: u16,
        row: u16,
    ) -> Option<Vec<u8>> {
        match mode {
            MouseMode::None => None,
            MouseMode::X10 => self.encode_x10(event, col, row),
            MouseMode::Normal => self.encode_normal(event, col, row),
            MouseMode::ButtonMotion => self.encode_button_motion(event, col, row),
            MouseMode::AnyMotion => self.encode_any_motion(event, col, row),
            MouseMode::Sgr => self.encode_sgr(event, col, row),
        }
    }

    /// Encode X10 mouse event (button press only)
    fn encode_x10(&self, event: MouseEvent, col: u16, row: u16) -> Option<Vec<u8>> {
        match event {
            MouseEvent::Press(button) => {
                let button_code = button.code();
                // X10: CSI M Cb Cx Cy (1-indexed, add 32 for printable)
                Some(vec![
                    0x1b, b'[', b'M',
                    32 + button_code,
                    32 + (col.min(222) as u8) + 1,
                    32 + (row.min(222) as u8) + 1,
                ])
            }
            _ => None,
        }
    }

    /// Encode normal mouse event (press + release)
    fn encode_normal(&self, event: MouseEvent, col: u16, row: u16) -> Option<Vec<u8>> {
        let button_code = match &event {
            MouseEvent::Press(b) => b.code(),
            MouseEvent::Release(_) => 3, // Release is button code 3
            _ => return None,
        };

        Some(vec![
            0x1b, b'[', b'M',
            32 + button_code,
            32 + (col.min(222) as u8) + 1,
            32 + (row.min(222) as u8) + 1,
        ])
    }

    /// Encode button-event mouse (press + release + drag)
    fn encode_button_motion(&self, event: MouseEvent, col: u16, row: u16) -> Option<Vec<u8>> {
        let button_code = match &event {
            MouseEvent::Press(b) => b.code(),
            MouseEvent::Release(_) => 3,
            MouseEvent::Drag(b) => b.code() + 32,
            MouseEvent::Move => return None,
        };

        Some(vec![
            0x1b, b'[', b'M',
            32 + button_code,
            32 + (col.min(222) as u8) + 1,
            32 + (row.min(222) as u8) + 1,
        ])
    }

    /// Encode any-event mouse (all motion events)
    fn encode_any_motion(&self, event: MouseEvent, col: u16, row: u16) -> Option<Vec<u8>> {
        let button_code = match &event {
            MouseEvent::Press(b) => b.code(),
            MouseEvent::Release(_) => 3,
            MouseEvent::Drag(b) => b.code() + 32,
            MouseEvent::Move => 32 + 3, // Motion with no button
        };

        Some(vec![
            0x1b, b'[', b'M',
            32 + button_code,
            32 + (col.min(222) as u8) + 1,
            32 + (row.min(222) as u8) + 1,
        ])
    }

    /// Encode SGR mouse event (extended coordinates)
    fn encode_sgr(&self, event: MouseEvent, col: u16, row: u16) -> Option<Vec<u8>> {
        let (button_code, release) = match &event {
            MouseEvent::Press(b) => (b.code(), false),
            MouseEvent::Release(b) => (b.code(), true),
            MouseEvent::Drag(b) => (b.code() + 32, false),
            MouseEvent::Move => (32 + 3, false),
        };

        // SGR: CSI < Cb ; Cx ; Cy M/m
        let suffix = if release { b'm' } else { b'M' };
        let seq = format!(
            "\x1b[<{};{};{}{}",
            button_code,
            col + 1, // 1-indexed
            row + 1,
            suffix as char,
        );
        Some(seq.into_bytes())
    }

    /// Handle selection based on click count
    pub fn handle_selection(
        &self,
        selection: &mut SelectionManager,
        grid: &Grid,
        col: u16,
        row: u16,
    ) {
        match self.click_count {
            1 => {
                // Single click - start normal selection
                selection.start(col, row, SelectionType::Normal);
            }
            2 => {
                // Double click - select word
                // Find word boundaries and start a word selection
                let (start_col, end_col) = self.find_word_bounds(grid, col, row);
                selection.start(start_col, row, SelectionType::Word);
                selection.update(end_col, row);
            }
            3 => {
                // Triple click - select line
                selection.start(0, row, SelectionType::Line);
                selection.update(grid.cols().saturating_sub(1), row);
            }
            _ => {}
        }
    }

    /// Find word boundaries at the given position
    fn find_word_bounds(&self, grid: &Grid, col: u16, row: u16) -> (u16, u16) {
        let cols = grid.cols();
        let mut start = col;
        let mut end = col;

        // Check if current cell is part of a word
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        let current_char = grid.cell(col, row).c;
        if !is_word_char(current_char) && current_char != ' ' {
            // Single punctuation character
            return (col, col);
        }

        // Expand left
        while start > 0 {
            let c = grid.cell(start - 1, row).c;
            if is_word_char(c) == is_word_char(current_char) {
                start -= 1;
            } else {
                break;
            }
        }

        // Expand right
        while end < cols - 1 {
            let c = grid.cell(end + 1, row).c;
            if is_word_char(c) == is_word_char(current_char) {
                end += 1;
            } else {
                break;
            }
        }

        (start, end)
    }

    /// Update selection during drag
    pub fn update_selection(&self, selection: &mut SelectionManager, col: u16, row: u16) {
        if self.dragging {
            selection.update(col, row);
        }
    }
}

impl Default for MouseHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_handler_new() {
        let handler = MouseHandler::new();
        assert_eq!(handler.cell_position(), (0, 0));
        assert!(!handler.is_dragging());
        assert!(handler.pressed_button().is_none());
    }

    #[test]
    fn pixel_to_cell_conversion() {
        let mut handler = MouseHandler::new();
        handler.set_cell_size(10.0, 20.0);

        assert_eq!(handler.pixel_to_cell(0.0, 0.0), (0, 0));
        assert_eq!(handler.pixel_to_cell(15.0, 25.0), (1, 1));
        assert_eq!(handler.pixel_to_cell(100.0, 200.0), (10, 10));
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
    fn mouse_press_release() {
        let mut handler = MouseHandler::new();

        handler.press(MouseButton::Left, 1);
        assert_eq!(handler.pressed_button(), Some(MouseButton::Left));
        assert_eq!(handler.click_count(), 1);

        handler.release(MouseButton::Left);
        assert!(handler.pressed_button().is_none());
    }

    #[test]
    fn mouse_drag_detection() {
        let mut handler = MouseHandler::new();
        handler.set_cell_size(10.0, 20.0);

        handler.update_position(5.0, 10.0);
        handler.press(MouseButton::Left, 1);
        assert!(!handler.is_dragging());

        // Move to different cell
        handler.update_position(25.0, 30.0);
        handler.motion();
        assert!(handler.is_dragging());
    }

    #[test]
    fn encode_x10_press() {
        let handler = MouseHandler::new();
        let encoded = handler.encode_event(
            MouseEvent::Press(MouseButton::Left),
            MouseMode::X10,
            5,
            10,
        );

        assert!(encoded.is_some());
        let bytes = encoded.unwrap();
        assert_eq!(bytes[0], 0x1b);
        assert_eq!(bytes[1], b'[');
        assert_eq!(bytes[2], b'M');
    }

    #[test]
    fn encode_x10_no_release() {
        let handler = MouseHandler::new();
        let encoded = handler.encode_event(
            MouseEvent::Release(MouseButton::Left),
            MouseMode::X10,
            5,
            10,
        );
        assert!(encoded.is_none());
    }

    #[test]
    fn encode_sgr_press() {
        let handler = MouseHandler::new();
        let encoded = handler.encode_event(
            MouseEvent::Press(MouseButton::Left),
            MouseMode::Sgr,
            5,
            10,
        );

        assert!(encoded.is_some());
        let seq = String::from_utf8(encoded.unwrap()).unwrap();
        assert!(seq.starts_with("\x1b[<"));
        assert!(seq.ends_with('M'));
        assert!(seq.contains(";6;")); // col + 1
        assert!(seq.contains(";11")); // row + 1
    }

    #[test]
    fn encode_sgr_release() {
        let handler = MouseHandler::new();
        let encoded = handler.encode_event(
            MouseEvent::Release(MouseButton::Left),
            MouseMode::Sgr,
            5,
            10,
        );

        assert!(encoded.is_some());
        let seq = String::from_utf8(encoded.unwrap()).unwrap();
        assert!(seq.ends_with('m')); // lowercase m for release
    }

    #[test]
    fn encode_none_mode() {
        let handler = MouseHandler::new();
        let encoded = handler.encode_event(
            MouseEvent::Press(MouseButton::Left),
            MouseMode::None,
            5,
            10,
        );
        assert!(encoded.is_none());
    }

    #[test]
    fn click_count_selection() {
        let mut handler = MouseHandler::new();

        handler.press(MouseButton::Left, 1);
        assert_eq!(handler.click_count(), 1);

        handler.press(MouseButton::Left, 2);
        assert_eq!(handler.click_count(), 2);

        handler.press(MouseButton::Left, 3);
        assert_eq!(handler.click_count(), 3);
    }
}
