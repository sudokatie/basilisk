//! ANSI escape sequence parsing
//!
//! VT100/xterm compatible parser for terminal control sequences.

pub mod parser;
pub mod handler;
pub mod csi;

pub use parser::{Parser, Action};
pub use handler::{Handler, Attr};
pub use csi::dispatch as csi_dispatch;
