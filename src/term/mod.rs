//! Terminal state management
//!
//! Contains cell representation, grid with scrollback, and cursor state.

pub mod cell;
pub mod grid;
pub mod cursor;
pub mod terminal;
pub mod selection;

pub use cell::{Cell, CellFlags, Color};
pub use grid::{Grid, Row};
pub use cursor::{Cursor, CursorShape, SavedCursor};
pub use terminal::Terminal;
pub use selection::{Selection, SelectionManager, SelectionType, Point};
