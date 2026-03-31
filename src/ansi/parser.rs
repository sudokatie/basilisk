//! ANSI/VT escape sequence parser

/// Actions emitted by the parser
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Print a character to the terminal
    Print(char),
    /// Execute a control code (C0/C1)
    Execute(u8),
    /// CSI sequence dispatch
    CsiDispatch {
        params: Vec<u16>,
        intermediates: Vec<u8>,
        action: char,
    },
    /// OSC sequence dispatch
    OscDispatch(Vec<Vec<u8>>),
    /// DCS sequence dispatch
    DcsDispatch {
        params: Vec<u16>,
        intermediates: Vec<u8>,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    OscString,
    DcsEntry,
    DcsParam,
    DcsPassthrough,
}

/// VT parser state machine
pub struct Parser {
    state: State,
    params: Vec<u16>,
    intermediates: Vec<u8>,
    osc_data: Vec<u8>,
    dcs_data: Vec<u8>,
    current_param: u16,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            params: Vec::new(),
            intermediates: Vec::new(),
            osc_data: Vec::new(),
            dcs_data: Vec::new(),
            current_param: 0,
        }
    }

    pub fn advance(&mut self, byte: u8) -> Option<Action> {
        match self.state {
            State::Ground => self.ground(byte),
            State::Escape => self.escape(byte),
            State::EscapeIntermediate => self.escape_intermediate(byte),
            State::CsiEntry => self.csi_entry(byte),
            State::CsiParam => self.csi_param(byte),
            State::CsiIntermediate => self.csi_intermediate(byte),
            State::OscString => self.osc_string(byte),
            State::DcsEntry => self.dcs_entry(byte),
            State::DcsParam => self.dcs_param(byte),
            State::DcsPassthrough => self.dcs_passthrough(byte),
        }
    }

    fn ground(&mut self, byte: u8) -> Option<Action> {
        match byte {
            0x1b => {
                self.state = State::Escape;
                None
            }
            0x00..=0x1f => Some(Action::Execute(byte)),
            0x20..=0x7e => Some(Action::Print(byte as char)),
            0x7f => None, // DEL ignored
            0x80..=0x9f => Some(Action::Execute(byte)),
            _ => {
                // UTF-8 handling would go here
                Some(Action::Print(byte as char))
            }
        }
    }

    fn escape(&mut self, byte: u8) -> Option<Action> {
        match byte {
            b'[' => {
                self.state = State::CsiEntry;
                self.params.clear();
                self.intermediates.clear();
                self.current_param = 0;
                None
            }
            b']' => {
                self.state = State::OscString;
                self.osc_data.clear();
                None
            }
            b'P' => {
                self.state = State::DcsEntry;
                self.params.clear();
                self.intermediates.clear();
                self.dcs_data.clear();
                self.current_param = 0;
                None
            }
            0x20..=0x2f => {
                self.state = State::EscapeIntermediate;
                self.intermediates.push(byte);
                None
            }
            0x30..=0x7e => {
                self.state = State::Ground;
                Some(Action::Execute(byte))
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn escape_intermediate(&mut self, byte: u8) -> Option<Action> {
        match byte {
            0x20..=0x2f => {
                self.intermediates.push(byte);
                None
            }
            0x30..=0x7e => {
                self.state = State::Ground;
                Some(Action::Execute(byte))
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn csi_entry(&mut self, byte: u8) -> Option<Action> {
        match byte {
            b'0'..=b'9' => {
                self.state = State::CsiParam;
                self.current_param = (byte - b'0') as u16;
                None
            }
            b';' => {
                self.params.push(0);
                self.state = State::CsiParam;
                None
            }
            0x3c..=0x3f => {
                self.intermediates.push(byte);
                None
            }
            0x40..=0x7e => {
                self.state = State::Ground;
                Some(Action::CsiDispatch {
                    params: std::mem::take(&mut self.params),
                    intermediates: std::mem::take(&mut self.intermediates),
                    action: byte as char,
                })
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn csi_param(&mut self, byte: u8) -> Option<Action> {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param.saturating_mul(10).saturating_add((byte - b'0') as u16);
                None
            }
            b';' => {
                self.params.push(self.current_param);
                self.current_param = 0;
                None
            }
            0x20..=0x2f => {
                self.params.push(self.current_param);
                self.current_param = 0;
                self.state = State::CsiIntermediate;
                self.intermediates.push(byte);
                None
            }
            0x40..=0x7e => {
                self.params.push(self.current_param);
                self.current_param = 0;
                self.state = State::Ground;
                Some(Action::CsiDispatch {
                    params: std::mem::take(&mut self.params),
                    intermediates: std::mem::take(&mut self.intermediates),
                    action: byte as char,
                })
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn csi_intermediate(&mut self, byte: u8) -> Option<Action> {
        match byte {
            0x20..=0x2f => {
                self.intermediates.push(byte);
                None
            }
            0x40..=0x7e => {
                self.state = State::Ground;
                Some(Action::CsiDispatch {
                    params: std::mem::take(&mut self.params),
                    intermediates: std::mem::take(&mut self.intermediates),
                    action: byte as char,
                })
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn osc_string(&mut self, byte: u8) -> Option<Action> {
        match byte {
            0x07 | 0x9c => {
                // BEL or ST terminates OSC
                self.state = State::Ground;
                let parts = self.osc_data.split(|&b| b == b';')
                    .map(|s| s.to_vec())
                    .collect();
                Some(Action::OscDispatch(parts))
            }
            0x1b => {
                // Check for ST (ESC \)
                self.state = State::Ground;
                let parts = self.osc_data.split(|&b| b == b';')
                    .map(|s| s.to_vec())
                    .collect();
                Some(Action::OscDispatch(parts))
            }
            _ => {
                self.osc_data.push(byte);
                None
            }
        }
    }

    fn dcs_entry(&mut self, byte: u8) -> Option<Action> {
        match byte {
            b'0'..=b'9' => {
                self.state = State::DcsParam;
                self.current_param = (byte - b'0') as u16;
                None
            }
            b';' => {
                self.params.push(0);
                self.state = State::DcsParam;
                None
            }
            0x40..=0x7e => {
                self.state = State::DcsPassthrough;
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn dcs_param(&mut self, byte: u8) -> Option<Action> {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param.saturating_mul(10).saturating_add((byte - b'0') as u16);
                None
            }
            b';' => {
                self.params.push(self.current_param);
                self.current_param = 0;
                None
            }
            0x40..=0x7e => {
                self.params.push(self.current_param);
                self.current_param = 0;
                self.state = State::DcsPassthrough;
                None
            }
            _ => {
                self.state = State::Ground;
                None
            }
        }
    }

    fn dcs_passthrough(&mut self, byte: u8) -> Option<Action> {
        match byte {
            0x9c => {
                // ST terminates DCS
                self.state = State::Ground;
                Some(Action::DcsDispatch {
                    params: std::mem::take(&mut self.params),
                    intermediates: std::mem::take(&mut self.intermediates),
                    data: std::mem::take(&mut self.dcs_data),
                })
            }
            0x1b => {
                // ESC might be start of ST
                self.state = State::Ground;
                Some(Action::DcsDispatch {
                    params: std::mem::take(&mut self.params),
                    intermediates: std::mem::take(&mut self.intermediates),
                    data: std::mem::take(&mut self.dcs_data),
                })
            }
            _ => {
                self.dcs_data.push(byte);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_print_char() {
        let mut parser = Parser::new();
        let action = parser.advance(b'A');
        assert_eq!(action, Some(Action::Print('A')));
    }

    #[test]
    fn parse_control_codes() {
        let mut parser = Parser::new();
        assert_eq!(parser.advance(b'\n'), Some(Action::Execute(b'\n')));
        assert_eq!(parser.advance(b'\r'), Some(Action::Execute(b'\r')));
        assert_eq!(parser.advance(b'\t'), Some(Action::Execute(b'\t')));
        assert_eq!(parser.advance(0x08), Some(Action::Execute(0x08))); // backspace
    }

    #[test]
    fn parse_escape_enters_escape_state() {
        let mut parser = Parser::new();
        assert_eq!(parser.advance(0x1b), None);
        assert_eq!(parser.state, State::Escape);
    }

    #[test]
    fn parse_csi_entry() {
        let mut parser = Parser::new();
        parser.advance(0x1b);
        assert_eq!(parser.advance(b'['), None);
        assert_eq!(parser.state, State::CsiEntry);
    }

    #[test]
    fn parse_csi_cursor_up() {
        let mut parser = Parser::new();
        parser.advance(0x1b);
        parser.advance(b'[');
        parser.advance(b'5');
        let action = parser.advance(b'A');
        assert_eq!(action, Some(Action::CsiDispatch {
            params: vec![5],
            intermediates: vec![],
            action: 'A',
        }));
    }

    #[test]
    fn parse_csi_sgr() {
        let mut parser = Parser::new();
        // ESC [ 1 ; 31 m (bold red)
        parser.advance(0x1b);
        parser.advance(b'[');
        parser.advance(b'1');
        parser.advance(b';');
        parser.advance(b'3');
        parser.advance(b'1');
        let action = parser.advance(b'm');
        assert_eq!(action, Some(Action::CsiDispatch {
            params: vec![1, 31],
            intermediates: vec![],
            action: 'm',
        }));
    }

    #[test]
    fn parse_csi_no_params() {
        let mut parser = Parser::new();
        parser.advance(0x1b);
        parser.advance(b'[');
        let action = parser.advance(b'H');
        assert_eq!(action, Some(Action::CsiDispatch {
            params: vec![],
            intermediates: vec![],
            action: 'H',
        }));
    }

    #[test]
    fn parse_osc_title() {
        let mut parser = Parser::new();
        parser.advance(0x1b);
        parser.advance(b']');
        for b in b"0;My Title" {
            parser.advance(*b);
        }
        let action = parser.advance(0x07); // BEL
        match action {
            Some(Action::OscDispatch(parts)) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0], b"0");
                assert_eq!(parts[1], b"My Title");
            }
            _ => panic!("Expected OscDispatch"),
        }
    }
}
