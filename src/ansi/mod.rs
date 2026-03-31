//! ANSI escape sequence parsing
//!
//! VT100/xterm compatible parser for terminal control sequences.

pub mod parser;

pub use parser::{Parser, Action};
