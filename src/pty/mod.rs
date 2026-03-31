//! Pseudo-terminal handling
//!
//! Platform-specific PTY implementation for spawning and managing shell processes.

#[cfg(unix)]
pub mod unix;

#[cfg(unix)]
pub use unix::Pty;
