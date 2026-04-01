//! Text selection for copy/paste

use super::grid::Grid;

/// Selection anchor point
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub col: u16,
    pub line: u16,
}

impl Point {
    pub fn new(col: u16, line: u16) -> Self {
        Self { col, line }
    }

    /// Check if this point is before another
    pub fn is_before(&self, other: &Point) -> bool {
        self.line < other.line || (self.line == other.line && self.col < other.col)
    }
}

/// Selection type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionType {
    /// Normal character-by-character selection
    Normal,
    /// Block/rectangular selection
    Block,
    /// Line selection (whole lines)
    Line,
    /// Word selection (double-click)
    Word,
}

/// Text selection state
#[derive(Debug, Clone)]
pub struct Selection {
    /// Selection type
    pub selection_type: SelectionType,
    /// Anchor point (where selection started)
    anchor: Point,
    /// Current point (where selection ends)
    current: Point,
    /// Whether selection is active
    active: bool,
}

impl Selection {
    /// Create a new selection at the given point
    pub fn new(col: u16, line: u16, selection_type: SelectionType) -> Self {
        let point = Point::new(col, line);
        Self {
            selection_type,
            anchor: point,
            current: point,
            active: true,
        }
    }

    /// Create a normal selection
    pub fn normal(col: u16, line: u16) -> Self {
        Self::new(col, line, SelectionType::Normal)
    }

    /// Create a block selection
    pub fn block(col: u16, line: u16) -> Self {
        Self::new(col, line, SelectionType::Block)
    }

    /// Create a line selection
    pub fn line(line: u16) -> Self {
        Self::new(0, line, SelectionType::Line)
    }

    /// Update the current position
    pub fn update(&mut self, col: u16, line: u16) {
        self.current = Point::new(col, line);
    }

    /// Get the start and end points (ordered)
    pub fn bounds(&self) -> (Point, Point) {
        if self.anchor.is_before(&self.current) {
            (self.anchor, self.current)
        } else {
            (self.current, self.anchor)
        }
    }

    /// Check if a cell is within the selection
    pub fn contains(&self, col: u16, line: u16) -> bool {
        let (start, end) = self.bounds();

        match self.selection_type {
            SelectionType::Normal | SelectionType::Word => {
                if line < start.line || line > end.line {
                    return false;
                }
                if line == start.line && line == end.line {
                    col >= start.col && col <= end.col
                } else if line == start.line {
                    col >= start.col
                } else if line == end.line {
                    col <= end.col
                } else {
                    true
                }
            }
            SelectionType::Block => {
                let (min_col, max_col) = if self.anchor.col < self.current.col {
                    (self.anchor.col, self.current.col)
                } else {
                    (self.current.col, self.anchor.col)
                };
                line >= start.line && line <= end.line && col >= min_col && col <= max_col
            }
            SelectionType::Line => {
                line >= start.line && line <= end.line
            }
        }
    }

    /// Check if selection is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Deactivate selection
    pub fn clear(&mut self) {
        self.active = false;
    }

    /// Extract selected text from grid
    pub fn extract_text(&self, grid: &Grid) -> String {
        if !self.active {
            return String::new();
        }

        let (start, end) = self.bounds();
        let mut result = String::new();

        match self.selection_type {
            SelectionType::Normal | SelectionType::Word => {
                for line in start.line..=end.line {
                    if line >= grid.lines() {
                        break;
                    }

                    let col_start = if line == start.line { start.col } else { 0 };
                    let col_end = if line == end.line { end.col } else { grid.cols().saturating_sub(1) };

                    for col in col_start..=col_end.min(grid.cols().saturating_sub(1)) {
                        let cell = grid.cell(col, line);
                        if cell.c != '\0' {
                            result.push(cell.c);
                        }
                    }

                    // Add newline between lines (but not at the very end)
                    if line < end.line {
                        // Trim trailing spaces before newline
                        while result.ends_with(' ') {
                            result.pop();
                        }
                        result.push('\n');
                    }
                }
            }
            SelectionType::Block => {
                let (min_col, max_col) = if self.anchor.col < self.current.col {
                    (self.anchor.col, self.current.col)
                } else {
                    (self.current.col, self.anchor.col)
                };

                for line in start.line..=end.line {
                    if line >= grid.lines() {
                        break;
                    }

                    for col in min_col..=max_col.min(grid.cols().saturating_sub(1)) {
                        let cell = grid.cell(col, line);
                        if cell.c != '\0' {
                            result.push(cell.c);
                        }
                    }

                    if line < end.line {
                        result.push('\n');
                    }
                }
            }
            SelectionType::Line => {
                for line in start.line..=end.line {
                    if line >= grid.lines() {
                        break;
                    }

                    for col in 0..grid.cols() {
                        let cell = grid.cell(col, line);
                        if cell.c != '\0' {
                            result.push(cell.c);
                        }
                    }

                    // Trim trailing spaces
                    while result.ends_with(' ') {
                        result.pop();
                    }

                    if line < end.line {
                        result.push('\n');
                    }
                }
            }
        }

        // Trim final trailing spaces
        while result.ends_with(' ') {
            result.pop();
        }

        result
    }
}

/// Selection manager for handling multiple selection operations
pub struct SelectionManager {
    selection: Option<Selection>,
}

impl SelectionManager {
    pub fn new() -> Self {
        Self { selection: None }
    }

    /// Start a new selection
    pub fn start(&mut self, col: u16, line: u16, selection_type: SelectionType) {
        self.selection = Some(Selection::new(col, line, selection_type));
    }

    /// Start a normal selection
    pub fn start_normal(&mut self, col: u16, line: u16) {
        self.start(col, line, SelectionType::Normal);
    }

    /// Update current selection
    pub fn update(&mut self, col: u16, line: u16) {
        if let Some(ref mut sel) = self.selection {
            sel.update(col, line);
        }
    }

    /// Clear selection
    pub fn clear(&mut self) {
        self.selection = None;
    }

    /// Get current selection
    pub fn get(&self) -> Option<&Selection> {
        self.selection.as_ref().filter(|s| s.is_active())
    }

    /// Check if a cell is selected
    pub fn is_selected(&self, col: u16, line: u16) -> bool {
        self.get().map(|s| s.contains(col, line)).unwrap_or(false)
    }

    /// Extract selected text
    pub fn extract_text(&self, grid: &Grid) -> Option<String> {
        self.get().map(|s| s.extract_text(grid))
    }

    /// Check if there's an active selection
    pub fn has_selection(&self) -> bool {
        self.get().is_some()
    }
}

impl Default for SelectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_is_before() {
        let p1 = Point::new(5, 3);
        let p2 = Point::new(10, 3);
        let p3 = Point::new(5, 5);

        assert!(p1.is_before(&p2)); // Same line, col before
        assert!(p1.is_before(&p3)); // Line before
        assert!(!p2.is_before(&p1));
    }

    #[test]
    fn selection_bounds() {
        let mut sel = Selection::normal(10, 5);
        sel.update(5, 3);

        let (start, end) = sel.bounds();
        assert_eq!(start.line, 3);
        assert_eq!(start.col, 5);
        assert_eq!(end.line, 5);
        assert_eq!(end.col, 10);
    }

    #[test]
    fn selection_contains_normal() {
        let mut sel = Selection::normal(5, 2);
        sel.update(10, 4);

        // Within selection
        assert!(sel.contains(7, 3));
        // Start line, after start col
        assert!(sel.contains(8, 2));
        // End line, before end col
        assert!(sel.contains(5, 4));
        // Outside
        assert!(!sel.contains(0, 0));
        assert!(!sel.contains(15, 4));
    }

    #[test]
    fn selection_contains_block() {
        let mut sel = Selection::block(5, 2);
        sel.update(10, 4);

        // Inside block
        assert!(sel.contains(7, 3));
        // Outside block column
        assert!(!sel.contains(3, 3));
        assert!(!sel.contains(12, 3));
    }

    #[test]
    fn selection_contains_line() {
        let mut sel = Selection::line(2);
        sel.update(0, 4);

        // Any column in selected lines
        assert!(sel.contains(0, 2));
        assert!(sel.contains(50, 3));
        assert!(sel.contains(100, 4));
        // Outside lines
        assert!(!sel.contains(0, 1));
        assert!(!sel.contains(0, 5));
    }

    #[test]
    fn selection_manager_workflow() {
        let mut manager = SelectionManager::new();

        assert!(!manager.has_selection());

        manager.start_normal(5, 3);
        assert!(manager.has_selection());

        manager.update(10, 5);
        assert!(manager.is_selected(7, 4));

        manager.clear();
        assert!(!manager.has_selection());
    }

    #[test]
    fn selection_extract_simple() {
        let mut grid = Grid::new(20, 5, 100);

        // Write "Hello" at line 1
        for (i, c) in "Hello".chars().enumerate() {
            grid.cell_mut(i as u16, 1).c = c;
        }

        let mut sel = Selection::normal(0, 1);
        sel.update(4, 1);

        let text = sel.extract_text(&grid);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn selection_extract_multiline() {
        let mut grid = Grid::new(20, 5, 100);

        for (i, c) in "Line1".chars().enumerate() {
            grid.cell_mut(i as u16, 0).c = c;
        }
        for (i, c) in "Line2".chars().enumerate() {
            grid.cell_mut(i as u16, 1).c = c;
        }

        let mut sel = Selection::normal(0, 0);
        sel.update(4, 1);

        let text = sel.extract_text(&grid);
        assert_eq!(text, "Line1\nLine2");
    }

    #[test]
    fn selection_clear() {
        let mut sel = Selection::normal(0, 0);
        assert!(sel.is_active());

        sel.clear();
        assert!(!sel.is_active());
    }
}
