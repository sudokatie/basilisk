//! ANSI escape sequence parsing
//!
//! VT100/xterm compatible parser for terminal control sequences.

pub mod parser;
pub mod handler;
pub mod csi;
pub mod osc;
pub mod sixel;
pub mod kitty;

pub use parser::{Parser, Action};
pub use handler::{Handler, Attr};
pub use csi::dispatch as csi_dispatch;
pub use osc::dispatch as osc_dispatch;
pub use sixel::{SixelDecoder, SixelImage, decode_sixel};
pub use kitty::{KittyDecoder, KittyImage, KittyCommand, KittyError};
