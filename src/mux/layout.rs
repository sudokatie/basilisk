//! Pane layout algorithms

use super::window::SplitDirection;

/// Rectangle representing pane position and size
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self { x, y, width, height }
    }
}

/// Layout tree for arranging panes
#[derive(Clone, Debug)]
pub enum Layout {
    /// A single pane
    Leaf(usize),
    /// Horizontal split (side by side)
    Horizontal(Vec<(f32, Layout)>),
    /// Vertical split (stacked)
    Vertical(Vec<(f32, Layout)>),
}

impl Layout {
    /// Create a leaf layout for a single pane
    pub fn leaf(pane_id: usize) -> Self {
        Layout::Leaf(pane_id)
    }

    /// Split a pane in the layout
    pub fn split(&self, target_id: usize, new_id: usize, direction: SplitDirection) -> Self {
        match self {
            Layout::Leaf(id) if *id == target_id => {
                let children = vec![
                    (0.5, Layout::Leaf(*id)),
                    (0.5, Layout::Leaf(new_id)),
                ];
                match direction {
                    SplitDirection::Horizontal => Layout::Horizontal(children),
                    SplitDirection::Vertical => Layout::Vertical(children),
                }
            }
            Layout::Leaf(_) => self.clone(),
            Layout::Horizontal(children) => {
                let new_children: Vec<_> = children
                    .iter()
                    .map(|(ratio, child)| (*ratio, child.split(target_id, new_id, direction)))
                    .collect();
                Layout::Horizontal(new_children)
            }
            Layout::Vertical(children) => {
                let new_children: Vec<_> = children
                    .iter()
                    .map(|(ratio, child)| (*ratio, child.split(target_id, new_id, direction)))
                    .collect();
                Layout::Vertical(new_children)
            }
        }
    }

    /// Remove a pane from the layout
    pub fn remove(&self, pane_id: usize) -> Self {
        match self {
            Layout::Leaf(id) => {
                if *id == pane_id {
                    // This shouldn't happen at top level, but return empty-ish
                    Layout::Leaf(0)
                } else {
                    self.clone()
                }
            }
            Layout::Horizontal(children) | Layout::Vertical(children) => {
                let new_children: Vec<_> = children
                    .iter()
                    .filter_map(|(ratio, child)| {
                        if matches!(child, Layout::Leaf(id) if *id == pane_id) {
                            None
                        } else {
                            Some((*ratio, child.remove(pane_id)))
                        }
                    })
                    .collect();

                if new_children.is_empty() {
                    Layout::Leaf(0)
                } else if new_children.len() == 1 {
                    new_children.into_iter().next().unwrap().1
                } else {
                    // Rebalance ratios
                    let total: f32 = new_children.iter().map(|(r, _)| r).sum();
                    let balanced: Vec<_> = new_children
                        .into_iter()
                        .map(|(r, c)| (r / total, c))
                        .collect();

                    match self {
                        Layout::Horizontal(_) => Layout::Horizontal(balanced),
                        Layout::Vertical(_) => Layout::Vertical(balanced),
                        _ => unreachable!(),
                    }
                }
            }
        }
    }

    /// Calculate positions for all panes in the layout
    pub fn calculate_positions(&self, bounds: &Rect) -> Vec<(usize, Rect)> {
        let mut result = Vec::new();
        self.calculate_positions_inner(bounds, &mut result);
        result
    }

    fn calculate_positions_inner(&self, bounds: &Rect, result: &mut Vec<(usize, Rect)>) {
        match self {
            Layout::Leaf(id) => {
                result.push((*id, *bounds));
            }
            Layout::Horizontal(children) => {
                let mut x = bounds.x;
                for (ratio, child) in children {
                    let width = (bounds.width as f32 * ratio) as u16;
                    let child_bounds = Rect {
                        x,
                        y: bounds.y,
                        width,
                        height: bounds.height,
                    };
                    child.calculate_positions_inner(&child_bounds, result);
                    x += width;
                }
            }
            Layout::Vertical(children) => {
                let mut y = bounds.y;
                for (ratio, child) in children {
                    let height = (bounds.height as f32 * ratio) as u16;
                    let child_bounds = Rect {
                        x: bounds.x,
                        y,
                        width: bounds.width,
                        height,
                    };
                    child.calculate_positions_inner(&child_bounds, result);
                    y += height;
                }
            }
        }
    }

    /// Count the number of panes in the layout
    pub fn pane_count(&self) -> usize {
        match self {
            Layout::Leaf(_) => 1,
            Layout::Horizontal(children) | Layout::Vertical(children) => {
                children.iter().map(|(_, c)| c.pane_count()).sum()
            }
        }
    }

    /// Check if layout contains a pane
    pub fn contains(&self, pane_id: usize) -> bool {
        match self {
            Layout::Leaf(id) => *id == pane_id,
            Layout::Horizontal(children) | Layout::Vertical(children) => {
                children.iter().any(|(_, c)| c.contains(pane_id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_new() {
        let rect = Rect::new(10, 20, 100, 50);
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 20);
        assert_eq!(rect.width, 100);
        assert_eq!(rect.height, 50);
    }

    #[test]
    fn layout_leaf() {
        let layout = Layout::leaf(42);
        assert!(matches!(layout, Layout::Leaf(42)));
        assert_eq!(layout.pane_count(), 1);
    }

    #[test]
    fn layout_contains() {
        let layout = Layout::leaf(1);
        assert!(layout.contains(1));
        assert!(!layout.contains(2));
    }

    #[test]
    fn layout_split_horizontal() {
        let layout = Layout::leaf(1);
        let split = layout.split(1, 2, SplitDirection::Horizontal);

        assert!(matches!(split, Layout::Horizontal(_)));
        assert_eq!(split.pane_count(), 2);
        assert!(split.contains(1));
        assert!(split.contains(2));
    }

    #[test]
    fn layout_split_vertical() {
        let layout = Layout::leaf(1);
        let split = layout.split(1, 2, SplitDirection::Vertical);

        assert!(matches!(split, Layout::Vertical(_)));
        assert_eq!(split.pane_count(), 2);
    }

    #[test]
    fn layout_calculate_positions() {
        let layout = Layout::leaf(1);
        let bounds = Rect::new(0, 0, 80, 24);
        let positions = layout.calculate_positions(&bounds);

        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], (1, bounds));
    }

    #[test]
    fn layout_calculate_positions_split() {
        let layout = Layout::leaf(1).split(1, 2, SplitDirection::Horizontal);
        let bounds = Rect::new(0, 0, 80, 24);
        let positions = layout.calculate_positions(&bounds);

        assert_eq!(positions.len(), 2);
        assert_eq!(positions[0].0, 1);
        assert_eq!(positions[0].1.width, 40);
        assert_eq!(positions[1].0, 2);
        assert_eq!(positions[1].1.x, 40);
    }

    #[test]
    fn layout_remove() {
        let layout = Layout::leaf(1)
            .split(1, 2, SplitDirection::Horizontal);
        let after = layout.remove(2);

        assert!(matches!(after, Layout::Leaf(1)));
    }

    #[test]
    fn layout_remove_rebalance() {
        let layout = Layout::Horizontal(vec![
            (0.33, Layout::Leaf(1)),
            (0.33, Layout::Leaf(2)),
            (0.34, Layout::Leaf(3)),
        ]);
        let after = layout.remove(2);

        // Should still have 2 panes with rebalanced ratios
        assert_eq!(after.pane_count(), 2);
    }
}
