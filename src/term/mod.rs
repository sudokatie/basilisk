//! Terminal state management
//!
//! Contains cell representation, grid with scrollback, and cursor state.

pub mod cell;
pub mod grid;
pub mod cursor;

pub use cell::{Cell, CellFlags, Color};
pub use grid::{Grid, Row};
pub use cursor::{Cursor, CursorShape, SavedCursor};
