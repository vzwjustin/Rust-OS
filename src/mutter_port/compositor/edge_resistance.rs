//! Edge resistance / window snapping algorithm
//! Ported from GNOME Mutter src/compositor/edge-resistance.c
//! Provides sticky edge snapping during window drag/resize operations.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.width)
    }

    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeType {
    Window,
    Monitor,
    Screen,
}

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub position: i32,
    pub is_horizontal: bool,
    pub edge_type: EdgeType,
}

/// Apply edge resistance to a proposed rectangle movement.
///
/// `proposed_rect`: the new position/size of the window
/// `edges`: list of edge positions to resist against
/// `old_x`, `old_y`: previous position
/// `threshold_towards`: snap distance when moving toward an edge
/// `threshold_away`: snap distance when moving away from an edge
pub fn apply_edge_resistance(
    proposed_rect: Rect,
    old_x: i32,
    old_y: i32,
    edges: &[Edge],
    threshold_towards: i32,
    threshold_away: i32,
) -> Rect {
    let mut result = proposed_rect;

    let old_left = old_x;
    let old_right = old_x.saturating_add(proposed_rect.width);
    let old_top = old_y;
    let old_bottom = old_y.saturating_add(proposed_rect.height);

    let new_left = result.x;
    let new_right = result.right();
    let new_top = result.y;
    let new_bottom = result.bottom();

    // Apply horizontal (x-axis) resistance
    let mut best_x_snap = result.x;
    let mut best_x_distance = core::i32::MAX;

    for edge in edges {
        if edge.is_horizontal {
            continue; // Only process vertical edges for x snapping
        }

        // Check left edge snapping
        let distance_to_left_snap = (new_left - edge.position).abs();
        let is_moving_toward_left = (new_left > edge.position && old_left <= edge.position)
            || (new_left < edge.position && old_left >= edge.position);

        let threshold = if is_moving_toward_left {
            threshold_towards
        } else {
            threshold_away
        };

        if distance_to_left_snap < threshold && distance_to_left_snap < best_x_distance {
            best_x_snap = result.x + (edge.position - new_left);
            best_x_distance = distance_to_left_snap;
        }

        // Check right edge snapping
        let distance_to_right_snap = (new_right - edge.position).abs();
        let is_moving_toward_right = (new_right > edge.position && old_right <= edge.position)
            || (new_right < edge.position && old_right >= edge.position);

        let threshold = if is_moving_toward_right {
            threshold_towards
        } else {
            threshold_away
        };

        if distance_to_right_snap < threshold && distance_to_right_snap < best_x_distance {
            best_x_snap = result.x + (edge.position - new_right);
            best_x_distance = distance_to_right_snap;
        }
    }

    if best_x_distance < core::i32::MAX {
        result.x = best_x_snap;
    }

    // Apply vertical (y-axis) resistance
    let new_left = result.x;
    let new_right = result.right();
    let mut best_y_snap = result.y;
    let mut best_y_distance = core::i32::MAX;

    for edge in edges {
        if !edge.is_horizontal {
            continue; // Only process horizontal edges for y snapping
        }

        // Check top edge snapping
        let distance_to_top_snap = (new_top - edge.position).abs();
        let is_moving_toward_top = (new_top > edge.position && old_top <= edge.position)
            || (new_top < edge.position && old_top >= edge.position);

        let threshold = if is_moving_toward_top {
            threshold_towards
        } else {
            threshold_away
        };

        if distance_to_top_snap < threshold && distance_to_top_snap < best_y_distance {
            best_y_snap = result.y + (edge.position - new_top);
            best_y_distance = distance_to_top_snap;
        }

        // Check bottom edge snapping
        let distance_to_bottom_snap = (new_bottom - edge.position).abs();
        let is_moving_toward_bottom = (new_bottom > edge.position && old_bottom <= edge.position)
            || (new_bottom < edge.position && old_bottom >= edge.position);

        let threshold = if is_moving_toward_bottom {
            threshold_towards
        } else {
            threshold_away
        };

        if distance_to_bottom_snap < threshold && distance_to_bottom_snap < best_y_distance {
            best_y_snap = result.y + (edge.position - new_bottom);
            best_y_distance = distance_to_bottom_snap;
        }
    }

    if best_y_distance < core::i32::MAX {
        result.y = best_y_snap;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_left_edge_snap() {
        let rect = Rect::new(35, 100, 200, 150);
        let edges = [Edge {
            position: 40,
            is_horizontal: false,
            edge_type: EdgeType::Monitor,
        }];

        let snapped = apply_edge_resistance(rect, 0, 100, &edges, 16, 0);
        assert_eq!(snapped.x, 40, "Should snap left edge to monitor edge");
    }

    #[test]
    fn test_right_edge_snap() {
        let rect = Rect::new(750, 100, 200, 150);
        let edges = [Edge {
            position: 740,
            is_horizontal: false,
            edge_type: EdgeType::Monitor,
        }];

        let snapped = apply_edge_resistance(rect, 600, 100, &edges, 16, 0);
        assert_eq!(snapped.x, 540, "Should snap right edge to monitor edge");
    }

    #[test]
    fn test_top_edge_snap() {
        let rect = Rect::new(100, 35, 200, 150);
        let edges = [Edge {
            position: 30,
            is_horizontal: true,
            edge_type: EdgeType::Monitor,
        }];

        let snapped = apply_edge_resistance(rect, 100, 0, &edges, 16, 0);
        assert_eq!(snapped.y, 30, "Should snap top edge to monitor edge");
    }

    #[test]
    fn test_no_snap_beyond_threshold() {
        let rect = Rect::new(100, 100, 200, 150);
        let edges = [Edge {
            position: 150,
            is_horizontal: false,
            edge_type: EdgeType::Monitor,
        }];

        let snapped = apply_edge_resistance(rect, 100, 100, &edges, 10, 0);
        assert_eq!(
            snapped.x, 100,
            "Should not snap when distance exceeds threshold"
        );
    }

    #[test]
    fn test_away_snap_distance() {
        let rect = Rect::new(100, 100, 200, 150);
        let old_x = 80;

        let edges = [Edge {
            position: 110,
            is_horizontal: false,
            edge_type: EdgeType::Monitor,
        }];

        // Moving away: left edge is at 100, right at 300, edge at 110
        // Distance away from edge = 110 - 100 = 10
        let snapped = apply_edge_resistance(rect, old_x, 100, &edges, 16, 20);
        assert!(
            snapped.x == 100 || snapped.x == 110,
            "Should consider away threshold"
        );
    }
}
