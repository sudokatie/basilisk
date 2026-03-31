//! Multiplexer (tmux-like functionality)
//!
//! Session, window, and pane management with flexible layouts.

pub mod pane;
pub mod layout;
pub mod window;
pub mod session;

pub use pane::{Pane, PaneId};
pub use window::{Window, WindowId, SplitDirection};
pub use layout::{Layout, Rect};
