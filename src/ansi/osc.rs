//! OSC (Operating System Command) sequence handling
//!
//! Handles terminal title, colors, clipboard, hyperlinks, and more.

use super::handler::Handler;
use crate::term::cell::Color;

/// OSC handler for extended terminal control
pub trait OscHandler: Handler {
    /// Set color palette entry (OSC 4)
    fn set_color(&mut self, index: u8, color: Color) { let _ = (index, color); }
    
    /// Query color palette entry (OSC 4)
    fn query_color(&mut self, _index: u8) -> Option<Color> { None }
    
    /// Set working directory (OSC 7)
    fn set_working_directory(&mut self, _path: &str) {}
    
    /// Start hyperlink (OSC 8)
    fn start_hyperlink(&mut self, _id: Option<&str>, _url: &str) {}
    
    /// End hyperlink (OSC 8)
    fn end_hyperlink(&mut self) {}
    
    /// Set foreground color (OSC 10)
    fn set_foreground_color(&mut self, _color: Color) {}
    
    /// Set background color (OSC 11)
    fn set_background_color(&mut self, _color: Color) {}
    
    /// Set cursor color (OSC 12)
    fn set_cursor_color(&mut self, _color: Color) {}
    
    /// Clipboard operation (OSC 52)
    fn clipboard_operation(&mut self, _selection: char, _data: Option<&str>) {}
    
    /// Reset color (OSC 104)
    fn reset_color(&mut self, _index: Option<u8>) {}
    
    /// Reset foreground color (OSC 110)
    fn reset_foreground_color(&mut self) {}
    
    /// Reset background color (OSC 111)
    fn reset_background_color(&mut self) {}
    
    /// Reset cursor color (OSC 112)
    fn reset_cursor_color(&mut self) {}
}

/// Dispatch an OSC sequence to the handler
pub fn dispatch<H: Handler>(handler: &mut H, params: &[Vec<u8>]) {
    if params.is_empty() {
        return;
    }

    // First param is the OSC code
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

        // OSC 4: Set/query color palette
        4 => {
            handle_palette_color(handler, &params[1..]);
        }

        // OSC 7: Set working directory (for shell integration)
        7 => {
            // Format: file://hostname/path
            if let Some(url) = params.get(1) {
                if let Ok(s) = std::str::from_utf8(url) {
                    if let Some(path) = s.strip_prefix("file://") {
                        // Strip hostname and get path
                        if let Some(idx) = path.find('/') {
                            handler.set_working_directory(&path[idx..]);
                        }
                    } else {
                        // Just a path without file:// prefix
                        handler.set_working_directory(s);
                    }
                }
            }
        }

        // OSC 8: Hyperlink
        8 => {
            // Format: OSC 8 ; params ; url ST
            // params can be empty or contain id=xxx
            // Empty url ends the hyperlink
            let params_str = params.get(0).and_then(|p| std::str::from_utf8(p).ok());
            let url = params.get(1).and_then(|u| std::str::from_utf8(u).ok());

            // Parse id from params
            let id = params_str.and_then(|p| {
                p.split(';')
                    .find(|s| s.starts_with("id="))
                    .map(|s| &s[3..])
            });

            handler.set_hyperlink(id, url);
        }

        // OSC 10: Set/query foreground color
        10 => {
            if let Some(color_spec) = params.get(1) {
                if let Some(color) = parse_color_spec(color_spec) {
                    // Can't call OscHandler methods through Handler trait
                    // This would need trait bounds adjustment
                    let _ = color;
                }
            }
        }

        // OSC 11: Set/query background color
        11 => {
            if let Some(color_spec) = params.get(1) {
                if let Some(color) = parse_color_spec(color_spec) {
                    let _ = color;
                }
            }
        }

        // OSC 12: Set/query cursor color
        12 => {
            if let Some(color_spec) = params.get(1) {
                if let Some(color) = parse_color_spec(color_spec) {
                    let _ = color;
                }
            }
        }

        // OSC 52: Clipboard manipulation
        52 => {
            handle_clipboard(handler, &params[1..]);
        }

        // OSC 104: Reset color palette
        104 => {
            // Reset specific color or all
        }

        // OSC 110: Reset foreground color
        110 => {}

        // OSC 111: Reset background color
        111 => {}

        // OSC 112: Reset cursor color
        112 => {}

        // OSC 133: Shell integration / prompt marking (iTerm2/FinalTerm)
        133 => {
            handle_shell_integration(handler, &params[1..]);
        }

        _ => {} // Unknown OSC, ignore
    }
}

/// Handle OSC 133 shell integration markers
fn handle_shell_integration<H: Handler>(_handler: &mut H, params: &[Vec<u8>]) {
    if params.is_empty() {
        return;
    }

    // Parse the marker type
    let marker = params.get(0)
        .and_then(|s| std::str::from_utf8(s).ok())
        .and_then(|s| s.chars().next());

    match marker {
        Some('A') => {
            // Prompt start - marks beginning of shell prompt
            // handler.shell_prompt_start();
        }
        Some('B') => {
            // Command start - user has typed command, cursor at command input
            // handler.shell_command_start();
        }
        Some('C') => {
            // Output start - command is running, output begins
            // handler.shell_output_start();
        }
        Some('D') => {
            // Command finished - parse exit code if present
            // Format: OSC 133 ; D ; <exit_code> ST
            let _exit_code = params.get(1)
                .and_then(|s| std::str::from_utf8(s).ok())
                .and_then(|s| s.parse::<i32>().ok());
            // handler.shell_command_finished(exit_code);
        }
        _ => {}
    }
}

/// Handle OSC 4 palette color
fn handle_palette_color<H: Handler>(_handler: &mut H, params: &[Vec<u8>]) {
    if params.is_empty() {
        return;
    }

    // Params come as pairs: index;spec or just index for query
    // Format: 4;index;spec or 4;index;?
    let mut i = 0;
    while i < params.len() {
        // Parse index
        let index = match std::str::from_utf8(&params[i]) {
            Ok(s) => match s.parse::<u8>() {
                Ok(n) => n,
                Err(_) => break,
            },
            Err(_) => break,
        };

        i += 1;
        if i >= params.len() {
            break;
        }

        let spec = &params[i];
        if spec == b"?" {
            // Query - would respond with current color
        } else if let Some(_color) = parse_color_spec(spec) {
            // Set color
            let _ = index;
        }

        i += 1;
    }
}

/// Handle OSC 8 hyperlink (currently handled inline in dispatch, kept for future use)
#[allow(dead_code)]
fn handle_hyperlink<H: Handler>(_handler: &mut H, params: &[Vec<u8>]) {
    if params.is_empty() {
        return;
    }

    // Format: params;url
    // params can be empty or contain id=xxx
    // Empty url ends the hyperlink
    
    let _params_str = params.get(0).and_then(|p| std::str::from_utf8(p).ok());
    let _url = params.get(1).and_then(|u| std::str::from_utf8(u).ok());

    // Parse id from params
    // if let Some(params) = params_str {
    //     let id = params.split(';')
    //         .find(|p| p.starts_with("id="))
    //         .map(|p| &p[3..]);
    // }
}

/// Handle OSC 52 clipboard
fn handle_clipboard<H: Handler>(handler: &mut H, params: &[Vec<u8>]) {
    if params.is_empty() {
        return;
    }

    // Format: selection;base64_data or selection;?
    // selection: c=clipboard, p=primary, q=secondary, s=select, 0-7=cut buffers
    
    let selection = params.get(0)
        .and_then(|s| std::str::from_utf8(s).ok())
        .and_then(|s| s.chars().next())
        .unwrap_or('c');

    let data = params.get(1)
        .and_then(|d| std::str::from_utf8(d).ok());

    match data {
        Some("?") => {
            // Query clipboard - pass None to indicate query
            handler.clipboard(selection, None);
        }
        Some(b64) if !b64.is_empty() => {
            // Set clipboard - pass base64 encoded data
            handler.clipboard(selection, Some(b64));
        }
        _ => {
            // Clear clipboard
            handler.clipboard(selection, Some(""));
        }
    }
}

/// Parse X11 color specification
fn parse_color_spec(spec: &[u8]) -> Option<Color> {
    let s = std::str::from_utf8(spec).ok()?;
    
    // Handle various formats:
    // rgb:RR/GG/BB or rgb:RRRR/GGGG/BBBB
    // #RRGGBB
    // color name (not supported here)
    
    if let Some(rgb) = s.strip_prefix("rgb:") {
        let parts: Vec<&str> = rgb.split('/').collect();
        if parts.len() == 3 {
            let r = u16::from_str_radix(parts[0], 16).ok()?;
            let g = u16::from_str_radix(parts[1], 16).ok()?;
            let b = u16::from_str_radix(parts[2], 16).ok()?;
            
            // Normalize to 8-bit
            let (r, g, b) = if parts[0].len() <= 2 {
                (r as u8, g as u8, b as u8)
            } else {
                ((r >> 8) as u8, (g >> 8) as u8, (b >> 8) as u8)
            };
            
            return Some(Color::rgb(r, g, b));
        }
    } else if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::rgb(r, g, b));
        }
    }
    
    None
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
        fn set_mode(&mut self, _mode: u16, _enable: bool) {}
        fn set_hyperlink(&mut self, _id: Option<&str>, _url: Option<&str>) {}
        fn set_working_directory(&mut self, _path: &str) {}
        fn clipboard(&mut self, _clipboard: char, _data: Option<&str>) {}
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
    fn parse_color_rgb() {
        let color = parse_color_spec(b"rgb:ff/00/80");
        assert!(color.is_some());
        let c = color.unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 128);
    }

    #[test]
    fn parse_color_hex() {
        let color = parse_color_spec(b"#ff0080");
        assert!(color.is_some());
        let c = color.unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 128);
    }

    #[test]
    fn parse_color_rgb_16bit() {
        let color = parse_color_spec(b"rgb:ffff/0000/8080");
        assert!(color.is_some());
        let c = color.unwrap();
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 128);
    }
}
