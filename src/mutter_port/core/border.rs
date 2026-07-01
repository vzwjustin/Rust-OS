//! Border and geometric utilities ported from GNOME Mutter (src/core/meta-border.c).
//!
//! Implements 2D vector and line intersection utilities for window motion constraints.
//! Used for detecting collisions and blocking directions during window movement.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-border.c

use core::f32;

/// 2D vector representation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector2 {
    /// X coordinate
    pub x: f32,
    /// Y coordinate
    pub y: f32,
}

impl Vector2 {
    /// Create a new 2D vector.
    pub fn new(x: f32, y: f32) -> Self {
        Vector2 { x, y }
    }

    /// Calculate the cross product of two vectors.
    #[inline]
    fn cross_product(a: Vector2, b: Vector2) -> f32 {
        a.x * b.y - a.y * b.x
    }

    /// Add two vectors.
    #[inline]
    fn add(a: Vector2, b: Vector2) -> Vector2 {
        Vector2 {
            x: a.x + b.x,
            y: a.y + b.y,
        }
    }

    /// Subtract two vectors.
    #[inline]
    fn subtract(a: Vector2, b: Vector2) -> Vector2 {
        Vector2 {
            x: a.x - b.x,
            y: a.y - b.y,
        }
    }

    /// Multiply a vector by a scalar constant.
    #[inline]
    fn multiply_constant(c: f32, a: Vector2) -> Vector2 {
        Vector2 {
            x: c * a.x,
            y: c * a.y,
        }
    }
}

/// 2D line segment representation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line2 {
    /// Start point
    pub a: Vector2,
    /// End point
    pub b: Vector2,
}

impl Line2 {
    /// Create a new line segment from two points.
    pub fn new(a: Vector2, b: Vector2) -> Self {
        Line2 { a, b }
    }

    /// Check if two line segments intersect and compute the intersection point.
    ///
    /// Uses parametric line equations and cross products to determine intersection.
    /// Lines only intersect if 0 ≤ t ≤ 1 and 0 ≤ u ≤ 1 along their parameters.
    ///
    /// # Arguments
    /// * `line1` - First line segment
    /// * `line2` - Second line segment
    /// * `intersection` - Output: intersection point if lines intersect
    ///
    /// # Returns
    /// true if the line segments intersect, false otherwise
    pub fn intersects_with(line1: &Line2, line2: &Line2) -> Option<Vector2> {
        let p = line1.a;
        let r = Vector2::subtract(line1.b, line1.a);
        let q = line2.a;
        let s = Vector2::subtract(line2.b, line2.a);

        let rxs = Vector2::cross_product(r, s);
        let _sxr = Vector2::cross_product(s, r);

        // If r × s ≈ 0 then the lines are parallel or collinear
        if rxs.abs() < f32::MIN_POSITIVE {
            return None;
        }

        let qp = Vector2::subtract(q, p);
        let pq = Vector2::subtract(p, q);

        let t = Vector2::cross_product(qp, s) / rxs;
        let u = Vector2::cross_product(pq, r) / rxs;

        // Lines only intersect if 0 ≤ t ≤ 1 and 0 ≤ u ≤ 1
        if t < 0.0 || t > 1.0 || u < 0.0 || u > 1.0 {
            return None;
        }

        Some(Vector2::add(p, Vector2::multiply_constant(t, r)))
    }
}

/// Direction flags for window motion constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionDirection(u32);

impl MotionDirection {
    /// Motion in positive X direction (rightward).
    pub const POSITIVE_X: u32 = 1 << 0;
    /// Motion in positive Y direction (downward).
    pub const POSITIVE_Y: u32 = 1 << 1;
    /// Motion in negative X direction (leftward).
    pub const NEGATIVE_X: u32 = 1 << 2;
    /// Motion in negative Y direction (upward).
    pub const NEGATIVE_Y: u32 = 1 << 3;

    /// Create a direction set from a bitmask.
    pub fn from_bits(bits: u32) -> Self {
        MotionDirection(bits)
    }

    /// Get the raw bitmask.
    pub fn bits(self) -> u32 {
        self.0
    }
}

/// Border constraint for window movement.
#[derive(Debug, Clone, Copy)]
pub struct Border {
    /// The line representing this border.
    pub line: Line2,
    /// Directions blocked by this border.
    blocking_directions: u32,
}

impl Border {
    /// Create a new border constraint.
    pub fn new(line: Line2, blocking_directions: u32) -> Self {
        Border {
            line,
            blocking_directions,
        }
    }

    /// Check if this border is horizontal (same Y coordinate for both endpoints).
    pub fn is_horizontal(&self) -> bool {
        self.line.a.y == self.line.b.y
    }

    /// Check if this border blocks the specified motion directions.
    ///
    /// A border blocks motion if it's perpendicular to the motion direction
    /// and the motion direction is not in the allows list.
    pub fn is_blocking_directions(&self, directions: MotionDirection) -> bool {
        let requested_dirs = directions.bits();

        if self.is_horizontal() {
            // Horizontal borders block vertical motion
            if (requested_dirs & (MotionDirection::POSITIVE_Y | MotionDirection::NEGATIVE_Y)) == 0 {
                return false;
            }
        } else {
            // Vertical borders block horizontal motion
            if (requested_dirs & (MotionDirection::POSITIVE_X | MotionDirection::NEGATIVE_X)) == 0 {
                return false;
            }
        }

        ((!self.blocking_directions) & requested_dirs) != requested_dirs
    }

    /// Get the directions this border allows (complement of blocking_directions).
    pub fn get_allows_directions(&self) -> u32 {
        let all_dirs = MotionDirection::POSITIVE_X
            | MotionDirection::POSITIVE_Y
            | MotionDirection::NEGATIVE_X
            | MotionDirection::NEGATIVE_Y;
        (!self.blocking_directions) & all_dirs
    }

    /// Set the directions this border allows.
    pub fn set_allows_directions(&mut self, directions: u32) {
        let all_dirs = MotionDirection::POSITIVE_X
            | MotionDirection::POSITIVE_Y
            | MotionDirection::NEGATIVE_X
            | MotionDirection::NEGATIVE_Y;
        self.blocking_directions = (!directions) & all_dirs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector2_creation() {
        let v = Vector2::new(1.0, 2.0);
        assert_eq!(v.x, 1.0);
        assert_eq!(v.y, 2.0);
    }

    #[test]
    fn test_vector2_add() {
        let a = Vector2::new(1.0, 2.0);
        let b = Vector2::new(3.0, 4.0);
        let result = Vector2::add(a, b);
        assert_eq!(result.x, 4.0);
        assert_eq!(result.y, 6.0);
    }

    #[test]
    fn test_vector2_subtract() {
        let a = Vector2::new(5.0, 7.0);
        let b = Vector2::new(2.0, 3.0);
        let result = Vector2::subtract(a, b);
        assert_eq!(result.x, 3.0);
        assert_eq!(result.y, 4.0);
    }

    #[test]
    fn test_line2_intersects() {
        let line1 = Line2::new(Vector2::new(0.0, 0.0), Vector2::new(2.0, 2.0));
        let line2 = Line2::new(Vector2::new(0.0, 2.0), Vector2::new(2.0, 0.0));

        if let Some(intersection) = Line2::intersects_with(&line1, &line2) {
            assert!((intersection.x - 1.0).abs() < 0.01);
            assert!((intersection.y - 1.0).abs() < 0.01);
        } else {
            panic!("Lines should intersect");
        }
    }

    #[test]
    fn test_line2_no_intersect() {
        let line1 = Line2::new(Vector2::new(0.0, 0.0), Vector2::new(1.0, 1.0));
        let line2 = Line2::new(Vector2::new(2.0, 2.0), Vector2::new(3.0, 3.0));

        assert!(Line2::intersects_with(&line1, &line2).is_none());
    }

    #[test]
    fn test_border_is_horizontal() {
        let border = Border::new(
            Line2::new(Vector2::new(0.0, 5.0), Vector2::new(10.0, 5.0)),
            0,
        );
        assert!(border.is_horizontal());
    }

    #[test]
    fn test_border_allows_directions() {
        let border = Border::new(
            Line2::new(Vector2::new(0.0, 5.0), Vector2::new(10.0, 5.0)),
            MotionDirection::POSITIVE_Y | MotionDirection::NEGATIVE_Y,
        );

        let allows = border.get_allows_directions();
        assert_eq!(
            allows & MotionDirection::POSITIVE_X,
            MotionDirection::POSITIVE_X
        );
        assert_eq!(
            allows & MotionDirection::NEGATIVE_X,
            MotionDirection::NEGATIVE_X
        );
    }
}
