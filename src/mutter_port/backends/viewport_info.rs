//! Viewport info ported from GNOME Mutter's src/backends/meta-viewport-info.c
//!
//! Tracks the set of logical monitor views (rectangle + scale) and provides
//! geometry queries: view-at-point, neighbor lookup, extents, and view counts.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-viewport-info.c

use alloc::vec::Vec;

/// Integer rectangle (mirrors `MtkRectangle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rectangle {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }

    /// Whether the floating-point point (x, y) is inside this rectangle.
    /// Mirrors `mtk_rectangle_contains_pointf`.
    pub fn contains_pointf(&self, x: f32, y: f32) -> bool {
        x >= self.x as f32
            && x < (self.x + self.width) as f32
            && y >= self.y as f32
            && y < (self.y + self.height) as f32
    }

    /// Whether the two rectangles overlap vertically (share Y extent).
    pub fn vert_overlap(&self, other: &Rectangle) -> bool {
        self.y < other.y + other.height && other.y < self.y + self.height
    }

    /// Whether the two rectangles overlap horizontally (share X extent).
    pub fn horiz_overlap(&self, other: &Rectangle) -> bool {
        self.x < other.x + other.width && other.x < self.x + self.width
    }
}

/// Direction to a neighboring display (mirrors `MetaDisplayDirection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayDirection {
    Up,
    Down,
    Left,
    Right,
}

/// A single view: its rectangle and scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ViewInfo {
    rect: Rectangle,
    scale: f32,
}

/// Immutable description of a set of monitor viewports.
#[derive(Debug, Clone)]
pub struct MetaViewportInfo {
    views: Vec<ViewInfo>,
    is_views_scaled: bool,
}

impl MetaViewportInfo {
    /// Construct from parallel arrays of rectangles and scales.
    pub fn new(views: &[Rectangle], scales: &[f32], is_views_scaled: bool) -> Self {
        let n = core::cmp::min(views.len(), scales.len());
        let mut infos = Vec::with_capacity(n);
        for i in 0..n {
            infos.push(ViewInfo {
                rect: views[i],
                scale: scales[i],
            });
        }
        MetaViewportInfo {
            views: infos,
            is_views_scaled,
        }
    }

    /// Index of the view containing point (x, y), or `None` if none.
    pub fn get_view_at(&self, x: f32, y: f32) -> Option<usize> {
        self.views
            .iter()
            .position(|info| info.rect.contains_pointf(x, y))
    }

    /// Rectangle and scale of the view at `idx`, or `None` if out of range.
    pub fn get_view_info(&self, idx: usize) -> Option<(Rectangle, f32)> {
        self.views.get(idx).map(|info| (info.rect, info.scale))
    }

    /// Index of the neighbor of view `idx` in `direction`, or `None`.
    pub fn get_neighbor(&self, idx: usize, direction: DisplayDirection) -> Option<usize> {
        let (rect, _) = self.get_view_info(idx)?;

        for (i, info) in self.views.iter().enumerate() {
            if i == idx {
                continue;
            }
            if view_has_neighbor(&rect, &info.rect, direction) {
                return Some(i);
            }
        }

        None
    }

    /// Number of views.
    pub fn get_num_views(&self) -> usize {
        self.views.len()
    }

    /// Combined width and height spanned by all views.
    pub fn get_extents(&self) -> (f32, f32) {
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for info in &self.views {
            min_x = min_x.min(info.rect.x);
            max_x = max_x.max(info.rect.x + info.rect.width);
            min_y = min_y.min(info.rect.y);
            max_y = max_y.max(info.rect.y + info.rect.height);
        }

        ((max_x - min_x) as f32, (max_y - min_y) as f32)
    }

    /// Whether the views are already expressed in scaled coordinates.
    pub fn is_views_scaled(&self) -> bool {
        self.is_views_scaled
    }
}

/// Whether `neighbor` is adjacent to `view` in `neighbor_direction`.
fn view_has_neighbor(
    view: &Rectangle,
    neighbor: &Rectangle,
    neighbor_direction: DisplayDirection,
) -> bool {
    match neighbor_direction {
        DisplayDirection::Right => {
            neighbor.x == (view.x + view.width) && neighbor.vert_overlap(view)
        }
        DisplayDirection::Left => {
            view.x == (neighbor.x + neighbor.width) && neighbor.vert_overlap(view)
        }
        DisplayDirection::Up => {
            view.y == (neighbor.y + neighbor.height) && neighbor.horiz_overlap(view)
        }
        DisplayDirection::Down => {
            neighbor.y == (view.y + view.height) && neighbor.horiz_overlap(view)
        }
    }
}
