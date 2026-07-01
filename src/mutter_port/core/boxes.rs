//! Geometry utilities ported from GNOME Mutter src/core/boxes.c
//! Rectangle operations: union, intersection, containment, clamping, edge alignment, strut handling

use crate::graphics::framebuffer::Rect;
use alloc::vec::Vec;

/// Check if two rectangles overlap (share any area)
pub fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

/// Get the intersection of two rectangles, or None if they don't overlap
pub fn rect_intersection(a: &Rect, b: &Rect) -> Option<Rect> {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);

    if left < right && top < bottom {
        Some(Rect::new(left, top, right - left, bottom - top))
    } else {
        None
    }
}

/// Get the bounding rectangle that contains both rectangles (union)
pub fn rect_union(a: &Rect, b: &Rect) -> Rect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);

    Rect::new(left, top, right - left, bottom - top)
}

/// Clamp a rectangle to fit within bounds, adjusting size if necessary
pub fn rect_clamp_to_fit_into(rect: &Rect, bounds: &Rect) -> Rect {
    let mut result = *rect;

    if result.width > bounds.width {
        result.width = bounds.width;
    }
    if result.height > bounds.height {
        result.height = bounds.height;
    }

    if result.x + result.width > bounds.x + bounds.width {
        result.x = (bounds.x + bounds.width).saturating_sub(result.width);
    }
    if result.x < bounds.x {
        result.x = bounds.x;
    }

    if result.y + result.height > bounds.y + bounds.height {
        result.y = (bounds.y + bounds.height).saturating_sub(result.height);
    }
    if result.y < bounds.y {
        result.y = bounds.y;
    }

    result
}

/// Check if a rectangle is fully contained within bounds
pub fn rect_is_contained_in(rect: &Rect, bounds: &Rect) -> bool {
    rect.x >= bounds.x
        && rect.x + rect.width <= bounds.x + bounds.width
        && rect.y >= bounds.y
        && rect.y + rect.height <= bounds.y + bounds.height
}

/// Check if a rectangle could fit entirely in a region (list of rectangles)
pub fn rect_could_fit_in_region(rect: &Rect, region: &[Rect]) -> bool {
    region.iter().any(|r| rect_is_contained_in(rect, r))
}

/// Get the area of overlap between two rectangles
pub fn rect_overlap_area(a: &Rect, b: &Rect) -> usize {
    if let Some(intersection) = rect_intersection(a, b) {
        intersection.area()
    } else {
        0
    }
}

/// Check if a rectangle is adjacent to any in a region (touching but not overlapping)
pub fn rect_is_adjacent_to_any(rect: &Rect, region: &[Rect]) -> bool {
    region.iter().any(|r| rect_is_adjacent(rect, r))
}

/// Check if two rectangles are adjacent (touching on an edge)
pub fn rect_is_adjacent(a: &Rect, b: &Rect) -> bool {
    let touches_vertically = (a.x + a.width == b.x || b.x + b.width == a.x)
        && a.y < b.y + b.height
        && a.y + a.height > b.y;

    let touches_horizontally = (a.y + a.height == b.y || b.y + b.height == a.y)
        && a.x < b.x + b.width
        && a.x + a.width > b.x;

    touches_vertically || touches_horizontally
}

/// Side enumeration for edges
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// Edge type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    Window,
    Monitor,
    Screen,
}

/// Represents a screen edge
#[derive(Debug, Clone)]
pub struct Edge {
    pub rect: Rect,
    pub side: Side,
    pub edge_type: EdgeType,
}

impl Edge {
    pub fn new(rect: Rect, side: Side, edge_type: EdgeType) -> Self {
        Self {
            rect,
            side,
            edge_type,
        }
    }

    /// Check if this edge aligns with a rectangle (in the way or adjacent)
    pub fn aligns_with(&self, rect: &Rect) -> bool {
        match self.side {
            Side::Left | Side::Right => {
                rect.y < self.rect.y + self.rect.height && self.rect.y < rect.y + rect.height
            }
            Side::Top | Side::Bottom => {
                rect.x < self.rect.x + self.rect.width && self.rect.x < rect.x + rect.width
            }
        }
    }
}

/// Represents a desktop strut (reserved space like panels)
#[derive(Debug, Clone)]
pub struct Strut {
    pub rect: Rect,
    pub side: Side,
}

impl Strut {
    pub fn new(rect: Rect, side: Side) -> Self {
        Self { rect, side }
    }
}

/// Expand a rectangle to avoid overlapping with struts on its side
pub fn rect_expand_to_avoiding_struts(rect: &mut Rect, struts: &[Strut], area_limit: &Rect) {
    for strut in struts {
        if rect_overlap_area(rect, &strut.rect) == 0 {
            continue;
        }

        match strut.side {
            Side::Left => {
                if rect.x > strut.rect.x + strut.rect.width {
                    let delta = rect.x - (strut.rect.x + strut.rect.width);
                    if rect.width + delta <= area_limit.width {
                        rect.width += delta;
                        rect.x -= delta;
                    }
                }
            }
            Side::Right => {
                if rect.x + rect.width < strut.rect.x {
                    let delta = strut.rect.x - (rect.x + rect.width);
                    if rect.width + delta <= area_limit.width {
                        rect.width += delta;
                    }
                }
            }
            Side::Top => {
                if rect.y > strut.rect.y + strut.rect.height {
                    let delta = rect.y - (strut.rect.y + strut.rect.height);
                    if rect.height + delta <= area_limit.height {
                        rect.height += delta;
                        rect.y -= delta;
                    }
                }
            }
            Side::Bottom => {
                if rect.y + rect.height < strut.rect.y {
                    let delta = strut.rect.y - (rect.y + rect.height);
                    if rect.height + delta <= area_limit.height {
                        rect.height += delta;
                    }
                }
            }
        }
    }
}

/// Find the closest point on the edge to a given coordinate
pub fn rect_find_linepoint_closest_to_point(edge: &Edge, x: usize, y: usize) -> (usize, usize) {
    match edge.side {
        Side::Left | Side::Right => {
            let edge_x = if edge.side == Side::Left {
                edge.rect.x
            } else {
                edge.rect.x + edge.rect.width
            };
            let y_clamped = y.max(edge.rect.y).min(edge.rect.y + edge.rect.height);
            (edge_x, y_clamped)
        }
        Side::Top | Side::Bottom => {
            let edge_y = if edge.side == Side::Top {
                edge.rect.y
            } else {
                edge.rect.y + edge.rect.height
            };
            let x_clamped = x.max(edge.rect.x).min(edge.rect.x + edge.rect.width);
            (x_clamped, edge_y)
        }
    }
}

/// Calculate the distance between a point and an edge
pub fn distance_to_edge(edge: &Edge, x: usize, y: usize) -> usize {
    let (px, py) = rect_find_linepoint_closest_to_point(edge, x, y);
    let dx = if x > px { x - px } else { px - x };
    let dy = if y > py { y - py } else { py - y };
    dx + dy
}

/// Find all edges from a list that don't intersect with any boxes
pub fn find_nonintersected_edges(edges: &[Edge], boxes: &[Rect]) -> Vec<Edge> {
    edges
        .iter()
        .filter(|edge| !boxes.iter().any(|b| rect_overlap_area(&edge.rect, b) > 0))
        .cloned()
        .collect()
}
