//! Basilisk - GPU-accelerated terminal emulator
//!
//! A modern terminal emulator built from scratch with GPU text rendering
//! via wgpu, achieving sub-millisecond frame times.

pub mod error;
pub mod term;
pub mod ansi;
pub mod pty;
pub mod mux;
pub mod render;
pub mod input;
pub mod config;
pub mod app;
pub mod clipboard;
pub mod window;
pub mod ssh;

pub use error::{Error, Result};
pub use app::App;
pub use clipboard::Clipboard;
