//! Port of GNOME mutter's `clutter/clutter-flow-layout.{c,h}`.
//!
//! A flow layout manager arranges children in rows/columns, wrapping when
//! out of space, with configurable row/column spacing and homogeneous sizing.
//! Implements the `LayoutManager` trait.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::cmp::max;

use super::actor::ActorId;
use super::actor_box::ActorBox;
use super::layout_manager::LayoutManager;

/// Orientation for layout direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// Main flow layout manager.
#[derive(Debug)]
pub struct FlowLayout {
    children: BTreeMap<ActorId, ()>,
    orientation: Orientation,
    col_spacing: f32,
    row_spacing: f32,
    min_col_width: f32,
    max_col_width: f32,
    col_width: f32,
    min_row_height: f32,
    max_row_height: f32,
    row_height: f32,
    line_min: Vec<f32>,
    line_natural: Vec<f32>,
    req_width: f32,
    req_height: f32,
    line_count: usize,
    is_homogeneous: bool,
    snap_to_grid: bool,
}

impl Default for FlowLayout {
    fn default() -> Self {
        Self::new(Orientation::Horizontal)
    }
}

impl FlowLayout {
    /// Create a new flow layout with given orientation.
    pub fn new(orientation: Orientation) -> Self {
        FlowLayout {
            children: BTreeMap::new(),
            orientation,
            col_spacing: 0.0,
            row_spacing: 0.0,
            min_col_width: 0.0,
            max_col_width: 0.0,
            col_width: 0.0,
            min_row_height: 0.0,
            max_row_height: 0.0,
            row_height: 0.0,
            line_min: Vec::new(),
            line_natural: Vec::new(),
            req_width: -1.0,
            req_height: -1.0,
            line_count: 0,
            is_homogeneous: false,
            snap_to_grid: false,
        }
    }

    /// Set layout orientation.
    pub fn set_orientation(&mut self, orientation: Orientation) {
        self.orientation = orientation;
    }

    /// Get current orientation.
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    /// Set whether all children have the same size.
    pub fn set_homogeneous(&mut self, homogeneous: bool) {
        self.is_homogeneous = homogeneous;
    }

    /// Get whether layout is homogeneous.
    pub fn homogeneous(&self) -> bool {
        self.is_homogeneous
    }

    /// Set spacing between columns.
    pub fn set_column_spacing(&mut self, spacing: f32) {
        self.col_spacing = spacing;
    }

    /// Get column spacing.
    pub fn column_spacing(&self) -> f32 {
        self.col_spacing
    }

    /// Set spacing between rows.
    pub fn set_row_spacing(&mut self, spacing: f32) {
        self.row_spacing = spacing;
    }

    /// Get row spacing.
    pub fn row_spacing(&self) -> f32 {
        self.row_spacing
    }

    /// Set minimum and maximum column width.
    pub fn set_column_width(&mut self, min_width: f32, max_width: f32) {
        self.min_col_width = min_width;
        self.max_col_width = max_width;
    }

    /// Get minimum and maximum column width.
    pub fn column_width(&self) -> (f32, f32) {
        (self.min_col_width, self.max_col_width)
    }

    /// Set minimum and maximum row height.
    pub fn set_row_height(&mut self, min_height: f32, max_height: f32) {
        self.min_row_height = min_height;
        self.max_row_height = max_height;
    }

    /// Get minimum and maximum row height.
    pub fn row_height(&self) -> (f32, f32) {
        (self.min_row_height, self.max_row_height)
    }

    /// Set whether to snap items to a grid.
    pub fn set_snap_to_grid(&mut self, snap_to_grid: bool) {
        self.snap_to_grid = snap_to_grid;
    }

    /// Get whether layout snaps to grid.
    pub fn snap_to_grid(&self) -> bool {
        self.snap_to_grid
    }

    /// Add a child to the layout.
    pub fn add_child(&mut self, actor: ActorId) {
        self.children.insert(actor, ());
    }

    /// Remove a child from the layout.
    pub fn remove_child(&mut self, actor: ActorId) {
        self.children.remove(&actor);
    }

    fn get_columns(&self, for_width: f32) -> i32 {
        if for_width < 0.0 {
            return 1;
        }
        if self.col_width == 0.0 {
            return 1;
        }
        let n_columns =
            ((for_width + self.col_spacing) / (self.col_width + self.col_spacing)) as i32;
        if n_columns == 0 {
            1
        } else {
            n_columns
        }
    }

    fn get_rows(&self, for_height: f32) -> i32 {
        if for_height < 0.0 {
            return 1;
        }
        if self.row_height == 0.0 {
            return 1;
        }
        let n_rows =
            ((for_height + self.row_spacing) / (self.row_height + self.row_spacing)) as i32;
        if n_rows == 0 {
            1
        } else {
            n_rows
        }
    }

    fn compute_lines(&self, avail_width: f32, avail_height: f32) -> i32 {
        match self.orientation {
            Orientation::Horizontal => self.get_columns(avail_width),
            Orientation::Vertical => self.get_rows(avail_height),
        }
    }
}

impl LayoutManager for FlowLayout {
    fn get_preferred_width(&self, _container: ActorId, _for_height: Option<f32>) -> (f32, f32) {
        let mut min_width = 0.0f32;
        let mut natural_width = 0.0f32;

        if !self.children.is_empty() {
            min_width = self.col_width.max(0.0);
            natural_width = self.col_width.max(0.0);

            let spacing = if self.line_count > 1 {
                (self.line_count as f32 - 1.0) * self.col_spacing
            } else {
                0.0
            };

            min_width += spacing;
            natural_width += spacing;
        }

        (min_width, natural_width)
    }

    fn get_preferred_height(&self, _container: ActorId, _for_width: Option<f32>) -> (f32, f32) {
        let mut min_height = 0.0f32;
        let mut natural_height = 0.0f32;

        if !self.children.is_empty() {
            min_height = self.row_height.max(0.0);
            natural_height = self.row_height.max(0.0);

            let spacing = if self.line_count > 1 {
                (self.line_count as f32 - 1.0) * self.row_spacing
            } else {
                0.0
            };

            min_height += spacing;
            natural_height += spacing;
        }

        (min_height, natural_height)
    }

    fn allocate(&mut self, _container: ActorId, allocation: &ActorBox) {
        if self.children.is_empty() {
            return;
        }

        let width = allocation.x2 - allocation.x1;
        let height = allocation.y2 - allocation.y1;
        let mut item_x = allocation.x1;
        let mut item_y = allocation.y1;
        let mut line_item_count = 0;
        let items_per_line = self.compute_lines(width, height);

        for _ in self.children.keys() {
            if self.orientation == Orientation::Horizontal {
                if self.snap_to_grid && line_item_count == items_per_line && line_item_count > 0 {
                    item_y += self.row_height + self.row_spacing;
                    line_item_count = 0;
                    item_x = allocation.x1;
                }

                let item_width = if self.snap_to_grid && items_per_line > 0 {
                    (width + self.col_spacing) / items_per_line as f32 - self.col_spacing
                } else {
                    self.col_width
                };

                let _ = ActorBox {
                    x1: item_x,
                    y1: item_y,
                    x2: (item_x + item_width).max(item_x),
                    y2: (item_y + self.row_height).max(item_y),
                };

                item_x += item_width + self.col_spacing;
                line_item_count += 1;
            } else {
                if self.snap_to_grid && line_item_count == items_per_line && line_item_count > 0 {
                    item_x += self.col_width + self.col_spacing;
                    line_item_count = 0;
                    item_y = allocation.y1;
                }

                let item_height = if self.snap_to_grid && items_per_line > 0 {
                    (height + self.row_spacing) / items_per_line as f32 - self.row_spacing
                } else {
                    self.row_height
                };

                let _ = ActorBox {
                    x1: item_x,
                    y1: item_y,
                    x2: (item_x + self.col_width).max(item_x),
                    y2: (item_y + item_height).max(item_y),
                };

                item_y += item_height + self.row_spacing;
                line_item_count += 1;
            }
        }
    }

    fn set_container(&mut self, _container: Option<ActorId>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_flow_layout() {
        let layout = FlowLayout::new(Orientation::Horizontal);
        assert_eq!(layout.orientation(), Orientation::Horizontal);
        assert_eq!(layout.column_spacing(), 0.0);
        assert_eq!(layout.row_spacing(), 0.0);
    }

    #[test]
    fn sets_orientation() {
        let mut layout = FlowLayout::new(Orientation::Horizontal);
        layout.set_orientation(Orientation::Vertical);
        assert_eq!(layout.orientation(), Orientation::Vertical);
    }

    #[test]
    fn adds_child() {
        let mut layout = FlowLayout::new(Orientation::Horizontal);
        let actor = ActorId::from_raw(1);
        layout.add_child(actor);
        assert_eq!(layout.children.len(), 1);
    }

    #[test]
    fn removes_child() {
        let mut layout = FlowLayout::new(Orientation::Horizontal);
        let actor = ActorId::from_raw(1);
        layout.add_child(actor);
        layout.remove_child(actor);
        assert_eq!(layout.children.len(), 0);
    }

    #[test]
    fn sets_spacing() {
        let mut layout = FlowLayout::new(Orientation::Horizontal);
        layout.set_column_spacing(5.0);
        layout.set_row_spacing(10.0);
        assert_eq!(layout.column_spacing(), 5.0);
        assert_eq!(layout.row_spacing(), 10.0);
    }

    #[test]
    fn sets_homogeneous() {
        let mut layout = FlowLayout::new(Orientation::Horizontal);
        layout.set_homogeneous(true);
        assert!(layout.homogeneous());
    }

    #[test]
    fn sets_snap_to_grid() {
        let mut layout = FlowLayout::new(Orientation::Horizontal);
        layout.set_snap_to_grid(true);
        assert!(layout.snap_to_grid());
    }

    #[test]
    fn sets_column_width() {
        let mut layout = FlowLayout::new(Orientation::Horizontal);
        layout.set_column_width(10.0, 100.0);
        let (min, max) = layout.column_width();
        assert_eq!(min, 10.0);
        assert_eq!(max, 100.0);
    }

    #[test]
    fn sets_row_height() {
        let mut layout = FlowLayout::new(Orientation::Horizontal);
        layout.set_row_height(20.0, 200.0);
        let (min, max) = layout.row_height();
        assert_eq!(min, 20.0);
        assert_eq!(max, 200.0);
    }
}
