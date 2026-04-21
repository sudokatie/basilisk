//! Window management and event loop
//!
//! Provides windowing abstraction over winit for the terminal display.

pub mod event_loop;

pub use event_loop::{run, WindowApp, WindowState};
