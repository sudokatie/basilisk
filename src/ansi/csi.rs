//! CSI (Control Sequence Introducer) sequence handling

use super::handler::{Attr, Handler};
use crate::term::cell::Color;

/// Dispatch a CSI sequence to the handler
pub fn dispatch<H: Handler>(
    handler: &mut H,
    params: &[u16],
    intermediates: &[u8],
    action: char,
) {
    let private_mode = intermediates.first() == Some(&b'?');

    match action {
        // Cursor movement
        'A' => handler.move_up(params.first().copied().unwrap_or(1).max(1)),
        'B' => handler.move_down(params.first().copied().unwrap_or(1).max(1)),
        'C' => handler.move_forward(params.first().copied().unwrap_or(1).max(1)),
        'D' => handler.move_backward(params.first().copied().unwrap_or(1).max(1)),
        'E' => handler.move_down_and_cr(params.first().copied().unwrap_or(1).max(1)),
        'F' => handler.move_up_and_cr(params.first().copied().unwrap_or(1).max(1)),
        'G' | '`' => handler.goto_col(params.first().copied().unwrap_or(1).saturating_sub(1)),
        'd' => handler.goto_line(params.first().copied().unwrap_or(1).saturating_sub(1)),
        'H' | 'f' => {
            let line = params.first().copied().unwrap_or(1).saturating_sub(1);
            let col = params.get(1).copied().unwrap_or(1).saturating_sub(1);
            handler.goto(line, col);
        }

        // Erase
        'J' => handler.erase_in_display(params.first().copied().unwrap_or(0)),
        'K' => handler.erase_in_line(params.first().copied().unwrap_or(0)),
        'X' => handler.erase_chars(params.first().copied().unwrap_or(1).max(1)),

        // Insert/Delete
        '@' => handler.insert_blank(params.first().copied().unwrap_or(1).max(1)),
        'P' => handler.delete_chars(params.first().copied().unwrap_or(1).max(1)),
        'L' => handler.insert_lines(params.first().copied().unwrap_or(1).max(1)),
        'M' => handler.delete_lines(params.first().copied().unwrap_or(1).max(1)),

        // Scrolling
        'S' => handler.scroll_up(params.first().copied().unwrap_or(1).max(1)),
        'T' => handler.scroll_down(params.first().copied().unwrap_or(1).max(1)),

        // Scroll region
        'r' => {
            let top = params.first().copied().unwrap_or(1);
            let bottom = params.get(1).copied().unwrap_or(0); // 0 means last line
            handler.set_scroll_region(top, bottom);
        }

        // Tab clear
        'g' => handler.clear_tab(params.first().copied().unwrap_or(0)),

        // SGR (Select Graphic Rendition)
        'm' => handle_sgr(handler, params),

        // Private modes (CSI ? Ps h/l)
        'h' if private_mode => {
            for &p in params {
                handler.set_mode(p, true);
            }
        }
        'l' if private_mode => {
            for &p in params {
                handler.set_mode(p, false);
            }
        }

        // Save/Restore cursor (DECSC/DECRC via CSI)
        's' => handler.save_cursor(),
        'u' => handler.restore_cursor(),

        _ => {} // Unknown sequence, ignore
    }
}

/// Handle SGR (Select Graphic Rendition) sequence
fn handle_sgr<H: Handler>(handler: &mut H, params: &[u16]) {
    if params.is_empty() {
        handler.set_attr(Attr::Reset);
        return;
    }

    let mut i = 0;
    while i < params.len() {
        match params[i] {
            0 => handler.set_attr(Attr::Reset),
            1 => handler.set_attr(Attr::Bold),
            2 => handler.set_attr(Attr::Dim),
            3 => handler.set_attr(Attr::Italic),
            4 => handler.set_attr(Attr::Underline),
            5 | 6 => handler.set_attr(Attr::Blink),
            7 => handler.set_attr(Attr::Inverse),
            8 => handler.set_attr(Attr::Hidden),
            9 => handler.set_attr(Attr::Strike),

            21 => handler.set_attr(Attr::CancelBold), // Also double underline
            22 => {
                handler.set_attr(Attr::CancelBold);
                handler.set_attr(Attr::CancelDim);
            }
            23 => handler.set_attr(Attr::CancelItalic),
            24 => handler.set_attr(Attr::CancelUnderline),
            25 => handler.set_attr(Attr::CancelBlink),
            27 => handler.set_attr(Attr::CancelInverse),
            28 => handler.set_attr(Attr::CancelHidden),
            29 => handler.set_attr(Attr::CancelStrike),

            // Foreground colors (8 standard)
            30..=37 => {
                let color = Color::from_ansi((params[i] - 30) as u8);
                handler.set_attr(Attr::Foreground(color));
            }
            38 => {
                if let Some(color) = parse_color(params, &mut i) {
                    handler.set_attr(Attr::Foreground(color));
                }
            }
            39 => handler.set_attr(Attr::DefaultForeground),

            // Background colors (8 standard)
            40..=47 => {
                let color = Color::from_ansi((params[i] - 40) as u8);
                handler.set_attr(Attr::Background(color));
            }
            48 => {
                if let Some(color) = parse_color(params, &mut i) {
                    handler.set_attr(Attr::Background(color));
                }
            }
            49 => handler.set_attr(Attr::DefaultBackground),

            // Bright foreground colors
            90..=97 => {
                let color = Color::from_256((params[i] - 90 + 8) as u8);
                handler.set_attr(Attr::Foreground(color));
            }

            // Bright background colors
            100..=107 => {
                let color = Color::from_256((params[i] - 100 + 8) as u8);
                handler.set_attr(Attr::Background(color));
            }

            _ => {} // Unknown, ignore
        }
        i += 1;
    }
}

/// Parse extended color (256-color or true color)
fn parse_color(params: &[u16], i: &mut usize) -> Option<Color> {
    if *i + 1 >= params.len() {
        return None;
    }

    match params[*i + 1] {
        // 256-color mode: 38;5;N or 48;5;N
        5 => {
            if *i + 2 < params.len() {
                *i += 2;
                Some(Color::from_256(params[*i] as u8))
            } else {
                None
            }
        }
        // True color mode: 38;2;R;G;B or 48;2;R;G;B
        2 => {
            if *i + 4 < params.len() {
                let r = params[*i + 2] as u8;
                let g = params[*i + 3] as u8;
                let b = params[*i + 4] as u8;
                *i += 4;
                Some(Color::rgb(r, g, b))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // Mock handler for testing
    struct MockHandler {
        calls: RefCell<Vec<String>>,
    }

    impl MockHandler {
        fn new() -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
            }
        }

        fn log(&self, s: &str) {
            self.calls.borrow_mut().push(s.to_string());
        }

        fn has_call(&self, s: &str) -> bool {
            self.calls.borrow().iter().any(|c| c.contains(s))
        }
    }

    impl Handler for MockHandler {
        fn input(&mut self, c: char) { self.log(&format!("input:{}", c)); }
        fn goto(&mut self, line: u16, col: u16) { self.log(&format!("goto:{},{}", line, col)); }
        fn goto_line(&mut self, line: u16) { self.log(&format!("goto_line:{}", line)); }
        fn goto_col(&mut self, col: u16) { self.log(&format!("goto_col:{}", col)); }
        fn move_up(&mut self, n: u16) { self.log(&format!("move_up:{}", n)); }
        fn move_down(&mut self, n: u16) { self.log(&format!("move_down:{}", n)); }
        fn move_forward(&mut self, n: u16) { self.log(&format!("move_forward:{}", n)); }
        fn move_backward(&mut self, n: u16) { self.log(&format!("move_backward:{}", n)); }
        fn move_down_and_cr(&mut self, n: u16) { self.log(&format!("move_down_cr:{}", n)); }
        fn move_up_and_cr(&mut self, n: u16) { self.log(&format!("move_up_cr:{}", n)); }
        fn insert_blank(&mut self, n: u16) { self.log(&format!("insert_blank:{}", n)); }
        fn newline(&mut self) { self.log("newline"); }
        fn carriage_return(&mut self) { self.log("cr"); }
        fn backspace(&mut self) { self.log("backspace"); }
        fn tab(&mut self) { self.log("tab"); }
        fn erase_chars(&mut self, n: u16) { self.log(&format!("erase_chars:{}", n)); }
        fn delete_chars(&mut self, n: u16) { self.log(&format!("delete_chars:{}", n)); }
        fn erase_in_line(&mut self, mode: u16) { self.log(&format!("erase_line:{}", mode)); }
        fn erase_in_display(&mut self, mode: u16) { self.log(&format!("erase_display:{}", mode)); }
        fn insert_lines(&mut self, n: u16) { self.log(&format!("insert_lines:{}", n)); }
        fn delete_lines(&mut self, n: u16) { self.log(&format!("delete_lines:{}", n)); }
        fn clear_tab(&mut self, mode: u16) { self.log(&format!("clear_tab:{}", mode)); }
        fn set_attr(&mut self, attr: Attr) { self.log(&format!("attr:{:?}", attr)); }
        fn set_title(&mut self, title: &str) { self.log(&format!("title:{}", title)); }
        fn bell(&mut self) { self.log("bell"); }
        fn reset(&mut self) { self.log("reset"); }
        fn save_cursor(&mut self) { self.log("save_cursor"); }
        fn restore_cursor(&mut self) { self.log("restore_cursor"); }
        fn set_scroll_region(&mut self, top: u16, bottom: u16) { self.log(&format!("scroll_region:{},{}", top, bottom)); }
        fn scroll_up(&mut self, n: u16) { self.log(&format!("scroll_up:{}", n)); }
        fn scroll_down(&mut self, n: u16) { self.log(&format!("scroll_down:{}", n)); }
        fn set_cursor_visible(&mut self, visible: bool) { self.log(&format!("cursor_visible:{}", visible)); }
        fn reverse_index(&mut self) { self.log("reverse_index"); }
        fn linefeed(&mut self) { self.log("linefeed"); }
        fn set_mode(&mut self, mode: u16, enable: bool) { self.log(&format!("set_mode:{}:{}", mode, enable)); }
        fn set_hyperlink(&mut self, id: Option<&str>, url: Option<&str>) { self.log(&format!("hyperlink:{:?}:{:?}", id, url)); }
        fn set_working_directory(&mut self, path: &str) { self.log(&format!("cwd:{}", path)); }
        fn clipboard(&mut self, clipboard: char, data: Option<&str>) { self.log(&format!("clipboard:{}:{:?}", clipboard, data)); }
    }

    #[test]
    fn csi_cursor_up() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[5], &[], 'A');
        assert!(h.has_call("move_up:5"));
    }

    #[test]
    fn csi_cursor_down() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[3], &[], 'B');
        assert!(h.has_call("move_down:3"));
    }

    #[test]
    fn csi_cursor_forward() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[2], &[], 'C');
        assert!(h.has_call("move_forward:2"));
    }

    #[test]
    fn csi_cursor_back() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[4], &[], 'D');
        assert!(h.has_call("move_backward:4"));
    }

    #[test]
    fn csi_cursor_position() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[10, 20], &[], 'H');
        assert!(h.has_call("goto:9,19")); // 1-indexed to 0-indexed
    }

    #[test]
    fn csi_cursor_position_default() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[], &[], 'H');
        assert!(h.has_call("goto:0,0"));
    }

    #[test]
    fn csi_erase_display() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[2], &[], 'J');
        assert!(h.has_call("erase_display:2"));
    }

    #[test]
    fn csi_erase_line() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[0], &[], 'K');
        assert!(h.has_call("erase_line:0"));
    }

    #[test]
    fn csi_sgr_reset() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[], &[], 'm');
        assert!(h.has_call("attr:Reset"));
    }

    #[test]
    fn csi_sgr_bold() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[1], &[], 'm');
        assert!(h.has_call("attr:Bold"));
    }

    #[test]
    fn csi_sgr_foreground() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[31], &[], 'm'); // Red
        assert!(h.has_call("Foreground"));
    }

    #[test]
    fn csi_sgr_256_color() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[38, 5, 196], &[], 'm'); // 256-color red
        assert!(h.has_call("Foreground"));
    }

    #[test]
    fn csi_sgr_true_color() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[38, 2, 255, 128, 0], &[], 'm'); // RGB orange
        assert!(h.has_call("Foreground"));
    }

    #[test]
    fn csi_scroll_up() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[5], &[], 'S');
        assert!(h.has_call("scroll_up:5"));
    }

    #[test]
    fn csi_delete_chars() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[3], &[], 'P');
        assert!(h.has_call("delete_chars:3"));
    }

    #[test]
    fn csi_cursor_visibility() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[25], &[b'?'], 'l'); // Hide cursor (DECTCEM)
        assert!(h.has_call("set_mode:25:false"));

        dispatch(&mut h, &[25], &[b'?'], 'h'); // Show cursor (DECTCEM)
        assert!(h.has_call("set_mode:25:true"));
    }

    #[test]
    fn csi_save_restore_cursor() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[], &[], 's');
        assert!(h.has_call("save_cursor"));

        dispatch(&mut h, &[], &[], 'u');
        assert!(h.has_call("restore_cursor"));
    }
}
