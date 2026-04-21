//! Terminal grid with scrollback

use std::collections::VecDeque;
use super::cell::Cell;

/// A single row of cells
#[derive(Clone, Debug)]
pub struct Row {
    pub cells: Vec<Cell>,
}

impl Row {
    pub fn new(cols: usize) -> Self {
        Self {
            cells: vec![Cell::empty(); cols],
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.reset();
        }
    }

    pub fn resize(&mut self, cols: usize) {
        self.cells.resize(cols, Cell::empty());
    }
}

/// Terminal grid with scrollback buffer
pub struct Grid {
    rows: Vec<Row>,
    scrollback: VecDeque<Row>,
    cols: u16,
    lines: u16,
    scrollback_limit: usize,
}

impl Grid {
    pub fn new(cols: u16, lines: u16, scrollback_limit: usize) -> Self {
        let rows = (0..lines).map(|_| Row::new(cols as usize)).collect();
        Self {
            rows,
            scrollback: VecDeque::new(),
            cols,
            lines,
            scrollback_limit,
        }
    }

    pub fn resize(&mut self, cols: u16, lines: u16) {
        // Resize existing rows
        for row in &mut self.rows {
            row.resize(cols as usize);
        }

        // Add or remove rows
        match lines.cmp(&self.lines) {
            std::cmp::Ordering::Greater => {
                for _ in 0..(lines - self.lines) {
                    self.rows.push(Row::new(cols as usize));
                }
            }
            std::cmp::Ordering::Less => {
                for _ in 0..(self.lines - lines) {
                    if !self.rows.is_empty() {
                        let row = self.rows.remove(0);
                        if self.scrollback.len() < self.scrollback_limit {
                            self.scrollback.push_back(row);
                        }
                    }
                }
            }
            std::cmp::Ordering::Equal => {}
        }

        self.cols = cols;
        self.lines = lines;
    }

    pub fn cell(&self, col: u16, line: u16) -> &Cell {
        &self.rows[line as usize].cells[col as usize]
    }

    pub fn cell_mut(&mut self, col: u16, line: u16) -> &mut Cell {
        &mut self.rows[line as usize].cells[col as usize]
    }

    pub fn scroll_up(&mut self, amount: usize) {
        for _ in 0..amount {
            let row = self.rows.remove(0);
            if self.scrollback.len() >= self.scrollback_limit {
                self.scrollback.pop_front();
            }
            self.scrollback.push_back(row);
            self.rows.push(Row::new(self.cols as usize));
        }
    }

    /// Scroll up within a region (for scroll regions)
    pub fn scroll_region_up(&mut self, top: u16, bottom: u16, amount: usize) {
        let top = top as usize;
        let bottom = bottom as usize;
        
        if top >= bottom || bottom >= self.rows.len() {
            return;
        }

        for _ in 0..amount {
            // Remove the top row of the region
            let row = self.rows.remove(top);
            
            // If scrolling the full screen from top, save to scrollback
            if top == 0 {
                if self.scrollback.len() >= self.scrollback_limit {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(row);
            }
            
            // Insert a new blank row at the bottom of the region
            self.rows.insert(bottom, Row::new(self.cols as usize));
        }
    }

    /// Scroll down within a region (for scroll regions)
    pub fn scroll_region_down(&mut self, top: u16, bottom: u16, amount: usize) {
        let top = top as usize;
        let bottom = bottom as usize;
        
        if top >= bottom || bottom >= self.rows.len() {
            return;
        }

        for _ in 0..amount {
            // Remove the bottom row of the region
            self.rows.remove(bottom);
            
            // Insert a new blank row at the top of the region
            self.rows.insert(top, Row::new(self.cols as usize));
        }
    }

    pub fn scroll_down(&mut self, amount: usize) {
        for _ in 0..amount {
            if let Some(row) = self.scrollback.pop_back() {
                self.rows.pop();
                self.rows.insert(0, row);
            }
        }
    }

    pub fn clear(&mut self) {
        for row in &mut self.rows {
            row.clear();
        }
    }

    pub fn clear_line(&mut self, line: u16) {
        if (line as usize) < self.rows.len() {
            self.rows[line as usize].clear();
        }
    }

    pub fn clear_region(&mut self, start: (u16, u16), end: (u16, u16)) {
        let (start_col, start_line) = start;
        let (end_col, end_line) = end;

        for line in start_line..=end_line {
            if (line as usize) >= self.rows.len() {
                break;
            }
            let col_start = if line == start_line { start_col } else { 0 };
            let col_end = if line == end_line { end_col } else { self.cols };

            for col in col_start..col_end {
                self.rows[line as usize].cells[col as usize].reset();
            }
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn lines(&self) -> u16 {
        self.lines
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn scrollback_row(&self, offset: usize) -> Option<&Row> {
        self.scrollback.get(self.scrollback.len().saturating_sub(offset + 1))
    }

    pub fn row(&self, line: u16) -> &Row {
        &self.rows[line as usize]
    }

    pub fn row_mut(&mut self, line: u16) -> &mut Row {
        &mut self.rows[line as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_new() {
        let grid = Grid::new(80, 24, 1000);
        assert_eq!(grid.cols(), 80);
        assert_eq!(grid.lines(), 24);
    }

    #[test]
    fn grid_cell_access() {
        let mut grid = Grid::new(80, 24, 1000);
        grid.cell_mut(0, 0).set_char('A');
        assert_eq!(grid.cell(0, 0).c(), 'A');
    }

    #[test]
    fn grid_resize_expand() {
        let mut grid = Grid::new(80, 24, 1000);
        grid.resize(100, 30);
        assert_eq!(grid.cols(), 100);
        assert_eq!(grid.lines(), 30);
    }

    #[test]
    fn grid_resize_shrink() {
        let mut grid = Grid::new(80, 24, 1000);
        grid.cell_mut(0, 0).set_char('X');
        grid.resize(60, 20);
        assert_eq!(grid.cols(), 60);
        assert_eq!(grid.lines(), 20);
        // First rows moved to scrollback
        assert!(grid.scrollback_len() > 0);
    }

    #[test]
    fn grid_scroll_up() {
        let mut grid = Grid::new(80, 24, 1000);
        grid.cell_mut(0, 0).set_char('A');
        grid.scroll_up(1);
        // First row moved to scrollback
        assert_eq!(grid.scrollback_len(), 1);
        // New first row is empty
        assert_eq!(grid.cell(0, 0).c(), ' ');
    }

    #[test]
    fn grid_scrollback_limit() {
        let mut grid = Grid::new(80, 24, 5);
        for i in 0..10 {
            grid.cell_mut(0, 0).set_char(char::from(b'A' + i));
            grid.scroll_up(1);
        }
        // Scrollback limited to 5
        assert_eq!(grid.scrollback_len(), 5);
    }

    #[test]
    fn grid_clear_line() {
        let mut grid = Grid::new(80, 24, 1000);
        grid.cell_mut(0, 5).set_char('X');
        grid.clear_line(5);
        assert_eq!(grid.cell(0, 5).c(), ' ');
    }

    #[test]
    fn grid_clear_region() {
        let mut grid = Grid::new(80, 24, 1000);
        for col in 0..10 {
            grid.cell_mut(col, 0).set_char('X');
        }
        grid.clear_region((2, 0), (7, 0));
        assert_eq!(grid.cell(0, 0).c(), 'X');
        assert_eq!(grid.cell(1, 0).c(), 'X');
        assert_eq!(grid.cell(2, 0).c(), ' ');
        assert_eq!(grid.cell(6, 0).c(), ' ');
        assert_eq!(grid.cell(7, 0).c(), 'X');
    }
}
