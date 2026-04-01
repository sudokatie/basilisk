//! Scrollback view for terminal history navigation

use super::grid::Grid;

/// Scrollback viewport state
pub struct ScrollbackView {
    /// Current scroll offset from bottom (0 = showing live terminal)
    offset: usize,
    /// Number of visible lines
    visible_lines: u16,
    /// Whether in scrollback mode (vs live mode)
    active: bool,
}

impl ScrollbackView {
    /// Create a new scrollback view
    pub fn new(visible_lines: u16) -> Self {
        Self {
            offset: 0,
            visible_lines,
            active: false,
        }
    }

    /// Get current scroll offset
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Check if in scrollback mode
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Set visible lines (on resize)
    pub fn set_visible_lines(&mut self, lines: u16) {
        self.visible_lines = lines;
    }

    /// Get visible lines
    pub fn visible_lines(&self) -> u16 {
        self.visible_lines
    }

    /// Enter scrollback mode
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Exit scrollback mode and return to live terminal
    pub fn deactivate(&mut self) {
        self.active = false;
        self.offset = 0;
    }

    /// Scroll up by n lines
    pub fn scroll_up(&mut self, n: usize, grid: &Grid) {
        let max_offset = grid.scrollback_len();
        self.offset = (self.offset + n).min(max_offset);
        if self.offset > 0 {
            self.active = true;
        }
    }

    /// Scroll down by n lines
    pub fn scroll_down(&mut self, n: usize) {
        if n >= self.offset {
            self.offset = 0;
            self.active = false;
        } else {
            self.offset -= n;
        }
    }

    /// Scroll up by one page
    pub fn page_up(&mut self, grid: &Grid) {
        let page_size = self.visible_lines.saturating_sub(1) as usize;
        self.scroll_up(page_size, grid);
    }

    /// Scroll down by one page
    pub fn page_down(&mut self) {
        let page_size = self.visible_lines.saturating_sub(1) as usize;
        self.scroll_down(page_size);
    }

    /// Scroll to top of scrollback
    pub fn scroll_to_top(&mut self, grid: &Grid) {
        self.offset = grid.scrollback_len();
        if self.offset > 0 {
            self.active = true;
        }
    }

    /// Scroll to bottom (live terminal)
    pub fn scroll_to_bottom(&mut self) {
        self.offset = 0;
        self.active = false;
    }

    /// Get the range of scrollback lines to display
    /// Returns (start_scrollback_idx, end_scrollback_idx, num_from_active)
    pub fn visible_range(&self, grid: &Grid) -> VisibleRange {
        let scrollback_len = grid.scrollback_len();
        let active_lines = grid.lines() as usize;
        let visible = self.visible_lines as usize;

        if self.offset == 0 {
            // Showing live terminal only
            return VisibleRange {
                scrollback_start: 0,
                scrollback_count: 0,
                active_start: 0,
                active_count: visible.min(active_lines),
            };
        }

        // How many lines of scrollback to show
        let scrollback_visible = self.offset.min(visible);
        // How many lines of active grid to show
        let active_visible = visible.saturating_sub(scrollback_visible);

        // Scrollback is indexed from most recent (0) to oldest
        // offset=1 means show the 1 most recent scrollback line at top
        let scrollback_start = scrollback_len.saturating_sub(self.offset);
        let scrollback_count = scrollback_visible.min(scrollback_len.saturating_sub(scrollback_start));

        VisibleRange {
            scrollback_start,
            scrollback_count,
            active_start: 0,
            active_count: active_visible.min(active_lines),
        }
    }

    /// Calculate scrollbar position (0.0 = top, 1.0 = bottom)
    pub fn scrollbar_position(&self, grid: &Grid) -> f32 {
        let max_offset = grid.scrollback_len();
        if max_offset == 0 {
            return 1.0;
        }
        1.0 - (self.offset as f32 / max_offset as f32)
    }

    /// Calculate scrollbar size (0.0 to 1.0)
    pub fn scrollbar_size(&self, grid: &Grid) -> f32 {
        let total = grid.scrollback_len() + grid.lines() as usize;
        if total == 0 {
            return 1.0;
        }
        (self.visible_lines as f32 / total as f32).min(1.0)
    }
}

/// Visible line range
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleRange {
    /// Starting index in scrollback buffer
    pub scrollback_start: usize,
    /// Number of lines from scrollback
    pub scrollback_count: usize,
    /// Starting line in active grid
    pub active_start: usize,
    /// Number of lines from active grid
    pub active_count: usize,
}

impl VisibleRange {
    /// Total visible lines
    pub fn total(&self) -> usize {
        self.scrollback_count + self.active_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid_with_scrollback(lines: u16, scrollback_lines: usize) -> Grid {
        let mut grid = Grid::new(80, lines, 10000);
        // Simulate scrollback by scrolling up multiple times
        for _ in 0..scrollback_lines {
            grid.scroll_up(1);
        }
        grid
    }

    #[test]
    fn scrollback_view_new() {
        let view = ScrollbackView::new(24);
        assert_eq!(view.offset(), 0);
        assert!(!view.is_active());
        assert_eq!(view.visible_lines(), 24);
    }

    #[test]
    fn scrollback_scroll_up() {
        let mut view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        view.scroll_up(10, &grid);
        assert_eq!(view.offset(), 10);
        assert!(view.is_active());
    }

    #[test]
    fn scrollback_scroll_down() {
        let mut view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        view.scroll_up(20, &grid);
        view.scroll_down(10);
        assert_eq!(view.offset(), 10);

        view.scroll_down(20); // More than offset
        assert_eq!(view.offset(), 0);
        assert!(!view.is_active());
    }

    #[test]
    fn scrollback_page_up_down() {
        let mut view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        view.page_up(&grid); // 23 lines
        assert_eq!(view.offset(), 23);

        view.page_down();
        assert_eq!(view.offset(), 0);
    }

    #[test]
    fn scrollback_to_top_bottom() {
        let mut view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        view.scroll_to_top(&grid);
        assert_eq!(view.offset(), 100);
        assert!(view.is_active());

        view.scroll_to_bottom();
        assert_eq!(view.offset(), 0);
        assert!(!view.is_active());
    }

    #[test]
    fn scrollback_max_offset() {
        let mut view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 50);

        view.scroll_up(1000, &grid); // Try to scroll past max
        assert_eq!(view.offset(), 50); // Clamped to scrollback length
    }

    #[test]
    fn scrollback_deactivate() {
        let mut view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        view.scroll_up(50, &grid);
        assert!(view.is_active());

        view.deactivate();
        assert!(!view.is_active());
        assert_eq!(view.offset(), 0);
    }

    #[test]
    fn visible_range_live() {
        let view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        let range = view.visible_range(&grid);
        assert_eq!(range.scrollback_count, 0);
        assert_eq!(range.active_count, 24);
    }

    #[test]
    fn visible_range_scrolled() {
        let mut view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        view.scroll_up(10, &grid);
        let range = view.visible_range(&grid);

        // With offset=10, we show 10 lines from scrollback and 14 from active
        assert_eq!(range.scrollback_count, 10);
        assert_eq!(range.active_count, 14);
    }

    #[test]
    fn scrollbar_position() {
        let mut view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        // At bottom
        assert!((view.scrollbar_position(&grid) - 1.0).abs() < 0.01);

        // At top
        view.scroll_to_top(&grid);
        assert!((view.scrollbar_position(&grid) - 0.0).abs() < 0.01);

        // Middle
        view.scroll_to_bottom();
        view.scroll_up(50, &grid);
        assert!((view.scrollbar_position(&grid) - 0.5).abs() < 0.01);
    }

    #[test]
    fn scrollbar_size() {
        let view = ScrollbackView::new(24);
        let grid = make_grid_with_scrollback(24, 100);

        // 24 visible out of 124 total
        let size = view.scrollbar_size(&grid);
        assert!((size - 24.0 / 124.0).abs() < 0.01);
    }
}
