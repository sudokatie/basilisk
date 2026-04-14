//! Multiplexer (tmux-like functionality)
//!
//! Session, window, and pane management with flexible layouts.

pub mod pane;
pub mod layout;
pub mod window;
pub mod session;
pub mod ipc;
pub mod multiplexer;

pub use pane::{Pane, PaneId};
pub use window::{Window, WindowId, SplitDirection};
pub use layout::{Layout, Rect};
pub use session::{Session, SessionId};
pub use ipc::{SessionServer, SessionClient, IpcMessage, list_sessions, session_socket_path};
pub use multiplexer::Multiplexer;
