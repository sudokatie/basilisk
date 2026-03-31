//! OSC (Operating System Command) sequence handling

use super::handler::Handler;

/// Dispatch an OSC sequence to the handler
pub fn dispatch<H: Handler>(handler: &mut H, params: &[Vec<u8>]) {
    if params.is_empty() {
        return;
    }

    // First param is the OSC code (e.g., "0", "1", "2", "8")
    let code = match std::str::from_utf8(&params[0]) {
        Ok(s) => s.parse::<u16>().unwrap_or(u16::MAX),
        Err(_) => return,
    };

    match code {
        // Set window title and icon name
        0 => {
            if let Some(title) = params.get(1) {
                if let Ok(s) = std::str::from_utf8(title) {
                    handler.set_title(s);
                }
            }
        }

        // Set icon name only
        1 => {
            // We treat this the same as title for simplicity
            if let Some(title) = params.get(1) {
                if let Ok(s) = std::str::from_utf8(title) {
                    handler.set_title(s);
                }
            }
        }

        // Set window title only
        2 => {
            if let Some(title) = params.get(1) {
                if let Ok(s) = std::str::from_utf8(title) {
                    handler.set_title(s);
                }
            }
        }

        // OSC 4: Set color palette (not implemented)
        4 => {}

        // OSC 7: Set working directory (not implemented)
        7 => {}

        // OSC 8: Hyperlink (params: id=..., url)
        8 => {
            // Hyperlinks not implemented yet, but structure is:
            // OSC 8 ; id=foo ; https://example.com ST
            // OSC 8 ; ; ST  (to end hyperlink)
        }

        // OSC 10: Set foreground color
        10 => {}

        // OSC 11: Set background color
        11 => {}

        // OSC 12: Set cursor color
        12 => {}

        // OSC 52: Clipboard manipulation (not implemented for security)
        52 => {}

        // OSC 104: Reset color palette
        104 => {}

        // OSC 112: Reset cursor color
        112 => {}

        _ => {} // Unknown OSC, ignore
    }
}

/// Parse OSC params from raw bytes (params are ';' separated)
pub fn parse_params(data: &[u8]) -> Vec<Vec<u8>> {
    data.split(|&b| b == b';').map(|s| s.to_vec()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct MockHandler {
        title: RefCell<String>,
    }

    impl MockHandler {
        fn new() -> Self {
            Self {
                title: RefCell::new(String::new()),
            }
        }
    }

    impl Handler for MockHandler {
        fn input(&mut self, _c: char) {}
        fn goto(&mut self, _line: u16, _col: u16) {}
        fn goto_line(&mut self, _line: u16) {}
        fn goto_col(&mut self, _col: u16) {}
        fn move_up(&mut self, _n: u16) {}
        fn move_down(&mut self, _n: u16) {}
        fn move_forward(&mut self, _n: u16) {}
        fn move_backward(&mut self, _n: u16) {}
        fn move_down_and_cr(&mut self, _n: u16) {}
        fn move_up_and_cr(&mut self, _n: u16) {}
        fn insert_blank(&mut self, _n: u16) {}
        fn newline(&mut self) {}
        fn carriage_return(&mut self) {}
        fn backspace(&mut self) {}
        fn tab(&mut self) {}
        fn erase_chars(&mut self, _n: u16) {}
        fn delete_chars(&mut self, _n: u16) {}
        fn erase_in_line(&mut self, _mode: u16) {}
        fn erase_in_display(&mut self, _mode: u16) {}
        fn insert_lines(&mut self, _n: u16) {}
        fn delete_lines(&mut self, _n: u16) {}
        fn clear_tab(&mut self, _mode: u16) {}
        fn set_attr(&mut self, _attr: super::super::handler::Attr) {}
        fn set_title(&mut self, title: &str) {
            *self.title.borrow_mut() = title.to_string();
        }
        fn bell(&mut self) {}
        fn reset(&mut self) {}
        fn save_cursor(&mut self) {}
        fn restore_cursor(&mut self) {}
        fn set_scroll_region(&mut self, _top: u16, _bottom: u16) {}
        fn scroll_up(&mut self, _n: u16) {}
        fn scroll_down(&mut self, _n: u16) {}
        fn set_cursor_visible(&mut self, _visible: bool) {}
        fn reverse_index(&mut self) {}
        fn linefeed(&mut self) {}
    }

    #[test]
    fn parse_params_empty() {
        let params = parse_params(b"");
        assert_eq!(params.len(), 1);
        assert!(params[0].is_empty());
    }

    #[test]
    fn parse_params_single() {
        let params = parse_params(b"0");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], b"0");
    }

    #[test]
    fn parse_params_multiple() {
        let params = parse_params(b"0;My Title");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], b"0");
        assert_eq!(params[1], b"My Title");
    }

    #[test]
    fn osc_set_title() {
        let mut h = MockHandler::new();
        let params = vec![b"0".to_vec(), b"Test Title".to_vec()];
        dispatch(&mut h, &params);
        assert_eq!(*h.title.borrow(), "Test Title");
    }

    #[test]
    fn osc_set_title_code_2() {
        let mut h = MockHandler::new();
        let params = vec![b"2".to_vec(), b"Window Title".to_vec()];
        dispatch(&mut h, &params);
        assert_eq!(*h.title.borrow(), "Window Title");
    }

    #[test]
    fn osc_icon_name() {
        let mut h = MockHandler::new();
        let params = vec![b"1".to_vec(), b"Icon".to_vec()];
        dispatch(&mut h, &params);
        assert_eq!(*h.title.borrow(), "Icon");
    }

    #[test]
    fn osc_empty_params() {
        let mut h = MockHandler::new();
        dispatch(&mut h, &[]);
        assert!(h.title.borrow().is_empty());
    }

    #[test]
    fn osc_invalid_code() {
        let mut h = MockHandler::new();
        let params = vec![b"999".to_vec(), b"ignored".to_vec()];
        dispatch(&mut h, &params);
        assert!(h.title.borrow().is_empty());
    }
}
