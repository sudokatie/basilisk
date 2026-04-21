//! Terminal state management
//!
//! Contains cell representation, grid with scrollback, and cursor state.

pub mod cell;
pub mod grid;
pub mod cursor;
pub mod terminal;
pub mod selection;
pub mod scrollback;
pub mod url;

pub use cell::{Cell, CellFlags, Color, ColorPalette, GraphemeStorage, SharedGraphemeStorage, new_grapheme_storage};
pub use grid::{Grid, Row};
pub use cursor::{Cursor, CursorShape, SavedCursor};
pub use terminal::{Terminal, TerminalModes, MouseMode, SixelPlacement, Hyperlink, StoredHyperlink, ClipboardRequest};
pub use selection::{Selection, SelectionManager, SelectionType, Point};
pub use scrollback::{ScrollbackView, VisibleRange};
