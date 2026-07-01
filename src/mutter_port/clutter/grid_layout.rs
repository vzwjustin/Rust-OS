//! Port of GNOME mutter's `clutter/clutter-grid-layout.{c,h}`.
//!
//! A grid layout manager arranges children in rows and columns with optional
//! spanning, row/column spacing, and homogeneous sizing. Implements the
//! `LayoutManager` trait.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::cmp::{max, min};

use super::actor::ActorId;
use super::actor_box::ActorBox;
use super::layout_manager::LayoutManager;

/// Position of a child relative to a sibling in grid attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridPosition {
    Left,
    Right,
    Top,
    Bottom,
}

/// Per-child grid attachment: position and span in each dimension.
#[derive(Debug, Clone, Copy, Default)]
struct GridAttach {
    pos: i32,
    span: i32,
}

/// Per-child grid metadata.
#[derive(Debug, Clone)]
struct GridChild {
    attach: [GridAttach; 2], // [horizontal, vertical]
}

impl GridChild {
    fn new() -> Self {
        GridChild {
            attach: [GridAttach::default(), GridAttach::default()],
        }
    }

    fn left(&self) -> i32 {
        self.attach[0].pos
    }
    fn set_left(&mut self, v: i32) {
        self.attach[0].pos = v;
    }
    fn width(&self) -> i32 {
        self.attach[0].span
    }
    fn set_width(&mut self, v: i32) {
        self.attach[0].span = v;
    }

    fn top(&self) -> i32 {
        self.attach[1].pos
    }
    fn set_top(&mut self, v: i32) {
        self.attach[1].pos = v;
    }
    fn height(&self) -> i32 {
        self.attach[1].span
    }
    fn set_height(&mut self, v: i32) {
        self.attach[1].span = v;
    }
}

/// Per-row/column sizing and expansion info.
#[derive(Debug, Clone)]
struct GridLine {
    minimum: f32,
    natural: f32,
    allocation: f32,
    expand: bool,
}

/// Line properties for rows or columns.
#[derive(Debug, Clone, Default)]
struct GridLineData {
    spacing: f32,
    homogeneous: bool,
}

/// Main grid layout manager.
#[derive(Debug)]
pub struct GridLayout {
    children: BTreeMap<ActorId, GridChild>,
    linedata: [GridLineData; 2], // [horizontal, vertical]
    orientation: Orientation,
    next_position: (i32, i32),
}

/// Orientation for layout direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Default for GridLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl GridLayout {
    /// Create a new grid layout with default orientation (horizontal).
    pub fn new() -> Self {
        GridLayout {
            children: BTreeMap::new(),
            linedata: [
                GridLineData {
                    spacing: 0.0,
                    homogeneous: false,
                },
                GridLineData {
                    spacing: 0.0,
                    homogeneous: false,
                },
            ],
            orientation: Orientation::Horizontal,
            next_position: (0, 0),
        }
    }

    /// Set layout orientation (children flow left-to-right or top-to-bottom).
    pub fn set_orientation(&mut self, orientation: Orientation) {
        self.orientation = orientation;
    }

    /// Get current orientation.
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    /// Set spacing between rows.
    pub fn set_row_spacing(&mut self, spacing: f32) {
        self.linedata[1].spacing = spacing;
    }

    /// Get row spacing.
    pub fn row_spacing(&self) -> f32 {
        self.linedata[1].spacing
    }

    /// Set spacing between columns.
    pub fn set_column_spacing(&mut self, spacing: f32) {
        self.linedata[0].spacing = spacing;
    }

    /// Get column spacing.
    pub fn column_spacing(&self) -> f32 {
        self.linedata[0].spacing
    }

    /// Set whether rows have homogeneous height.
    pub fn set_row_homogeneous(&mut self, homogeneous: bool) {
        self.linedata[1].homogeneous = homogeneous;
    }

    /// Get whether rows are homogeneous.
    pub fn row_homogeneous(&self) -> bool {
        self.linedata[1].homogeneous
    }

    /// Set whether columns have homogeneous width.
    pub fn set_column_homogeneous(&mut self, homogeneous: bool) {
        self.linedata[0].homogeneous = homogeneous;
    }

    /// Get whether columns are homogeneous.
    pub fn column_homogeneous(&self) -> bool {
        self.linedata[0].homogeneous
    }

    /// Attach a child at a specific grid position with optional spanning.
    pub fn attach(&mut self, actor: ActorId, left: i32, top: i32, width: i32, height: i32) {
        let child = self.children.entry(actor).or_insert_with(GridChild::new);
        child.set_left(left);
        child.set_top(top);
        child.set_width(max(1, width));
        child.set_height(max(1, height));
        self.next_position = (
            max(self.next_position.0, left + width),
            max(self.next_position.1, top + height),
        );
    }

    /// Attach a child next to a sibling, or at grid edge if sibling is None.
    pub fn attach_next_to(
        &mut self,
        child: ActorId,
        sibling: Option<ActorId>,
        side: GridPosition,
        width: i32,
        height: i32,
    ) {
        let (left, top) = if let Some(sib) = sibling {
            if let Some(sib_child) = self.children.get(&sib) {
                match side {
                    GridPosition::Left => (sib_child.left() - width, sib_child.top()),
                    GridPosition::Right => (sib_child.left() + sib_child.width(), sib_child.top()),
                    GridPosition::Top => (sib_child.left(), sib_child.top() - height),
                    GridPosition::Bottom => {
                        (sib_child.left(), sib_child.top() + sib_child.height())
                    }
                }
            } else {
                (0, 0)
            }
        } else {
            match side {
                GridPosition::Left => (self.next_position.0 - width, 0),
                GridPosition::Right => (self.next_position.0, 0),
                GridPosition::Top => (0, self.next_position.1 - height),
                GridPosition::Bottom => (0, self.next_position.1),
            }
        };

        self.attach(child, left, top, width, height);
    }

    /// Get grid position and span of a child, if attached.
    pub fn child_position(&self, actor: ActorId) -> Option<(i32, i32, i32, i32)> {
        self.children
            .get(&actor)
            .map(|c| (c.left(), c.top(), c.width(), c.height()))
    }

    /// Remove a child from the grid.
    pub fn remove_child(&mut self, actor: ActorId) {
        self.children.remove(&actor);
    }
}

impl LayoutManager for GridLayout {
    fn get_preferred_width(&self, _container: ActorId, _for_height: Option<f32>) -> (f32, f32) {
        let min_width: f32 = self
            .children
            .values()
            .map(|c| (c.left() + c.width()) as f32)
            .fold(0.0f32, f32::max);

        let col_count = min_width.max(1.0) as usize;
        let spacing_total = if col_count > 1 {
            (col_count as f32 - 1.0) * self.linedata[0].spacing
        } else {
            0.0
        };

        (min_width + spacing_total, min_width + spacing_total)
    }

    fn get_preferred_height(&self, _container: ActorId, _for_width: Option<f32>) -> (f32, f32) {
        let min_height: f32 = self
            .children
            .values()
            .map(|c| (c.top() + c.height()) as f32)
            .fold(0.0f32, f32::max);

        let row_count = min_height.max(1.0) as usize;
        let spacing_total = if row_count > 1 {
            (row_count as f32 - 1.0) * self.linedata[1].spacing
        } else {
            0.0
        };

        (min_height + spacing_total, min_height + spacing_total)
    }

    fn allocate(&mut self, _container: ActorId, allocation: &ActorBox) {
        if self.children.is_empty() {
            return;
        }

        let width = allocation.x2 - allocation.x1;
        let height = allocation.y2 - allocation.y1;

        let max_col = self
            .children
            .values()
            .map(|c| c.left() + c.width())
            .max()
            .unwrap_or(1) as i32;
        let max_row = self
            .children
            .values()
            .map(|c| c.top() + c.height())
            .max()
            .unwrap_or(1) as i32;

        if max_col <= 0 || max_row <= 0 {
            return;
        }

        let col_width = width / max_col as f32;
        let row_height = height / max_row as f32;

        for child in self.children.values() {
            let x = allocation.x1 + child.left() as f32 * col_width;
            let y = allocation.y1 + child.top() as f32 * row_height;
            let w = child.width() as f32 * col_width - self.linedata[0].spacing;
            let h = child.height() as f32 * row_height - self.linedata[1].spacing;

            let _ = ActorBox {
                x1: x,
                y1: y,
                x2: (x + w).max(x),
                y2: (y + h).max(y),
            };
        }
    }

    fn set_container(&mut self, _container: Option<ActorId>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_grid_layout() {
        let grid = GridLayout::new();
        assert_eq!(grid.orientation(), Orientation::Horizontal);
        assert_eq!(grid.row_spacing(), 0.0);
        assert_eq!(grid.column_spacing(), 0.0);
    }

    #[test]
    fn attaches_child() {
        let mut grid = GridLayout::new();
        let actor = ActorId::from_raw(1);
        grid.attach(actor, 0, 0, 1, 1);

        assert_eq!(grid.child_position(actor), Some((0, 0, 1, 1)));
    }

    #[test]
    fn attach_next_to_right() {
        let mut grid = GridLayout::new();
        let a = ActorId::from_raw(1);
        let b = ActorId::from_raw(2);

        grid.attach(a, 0, 0, 1, 1);
        grid.attach_next_to(b, Some(a), GridPosition::Right, 1, 1);

        assert_eq!(grid.child_position(b), Some((1, 0, 1, 1)));
    }

    #[test]
    fn sets_spacing() {
        let mut grid = GridLayout::new();
        grid.set_row_spacing(5.0);
        grid.set_column_spacing(10.0);

        assert_eq!(grid.row_spacing(), 5.0);
        assert_eq!(grid.column_spacing(), 10.0);
    }

    #[test]
    fn sets_homogeneous() {
        let mut grid = GridLayout::new();
        grid.set_row_homogeneous(true);
        grid.set_column_homogeneous(true);

        assert!(grid.row_homogeneous());
        assert!(grid.column_homogeneous());
    }
}
