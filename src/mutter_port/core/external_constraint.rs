//! External window constraint interface ported from GNOME Mutter (src/core/meta-external-constraint.c).
//!
//! Defines an interface for external geometry constraints on windows.
//! Allows third-party code to influence window positioning and sizing.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-external-constraint.c
//! Omitted: GObject interface machinery (G_DEFINE_INTERFACE, class_init, etc.)

use crate::desktop::window_manager::WindowId;

/// Rectangle representing window geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rectangle {
    /// X coordinate
    pub x: i32,
    /// Y coordinate
    pub y: i32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

impl Rectangle {
    /// Create a new rectangle.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if this rectangle intersects with another.
    pub fn intersects(&self, other: &Rectangle) -> bool {
        self.x < other.x + other.width as i32
            && self.x + self.width as i32 > other.x
            && self.y < other.y + other.height as i32
            && self.y + self.height as i32 > other.y
    }

    /// Get the area of this rectangle.
    pub fn area(&self) -> u64 {
        (self.width as u64) * (self.height as u64)
    }
}

/// Information about a constrained window operation.
#[derive(Debug, Clone)]
pub struct ConstraintInfo {
    /// The requested new rectangle.
    pub new_rect: Rectangle,
    /// The old rectangle (before the operation).
    pub old_rect: Rectangle,
    /// Whether the operation would cause a move.
    pub is_move_op: bool,
    /// Whether the operation would cause a resize.
    pub is_resize_op: bool,
}

impl ConstraintInfo {
    /// Create new constraint info.
    pub fn new(
        new_rect: Rectangle,
        old_rect: Rectangle,
        is_move_op: bool,
        is_resize_op: bool,
    ) -> Self {
        ConstraintInfo {
            new_rect,
            old_rect,
            is_move_op,
            is_resize_op,
        }
    }
}

/// Result of constraint evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintResult {
    /// Constraint is satisfied; operation can proceed.
    Allow,
    /// Constraint violation; operation should be adjusted.
    Adjust,
    /// Constraint failure; operation should not proceed.
    Deny,
}

/// Trait for external geometry constraints.
///
/// Implementors can enforce custom window positioning and sizing rules.
pub trait ExternalConstraint {
    /// Check if a window operation satisfies this constraint.
    ///
    /// # Arguments
    /// * `window_id` - The window being constrained
    /// * `info` - Information about the requested operation
    ///
    /// # Returns
    /// A ConstraintResult indicating whether the operation is allowed
    fn check_constraint(&self, window_id: WindowId, info: &ConstraintInfo) -> ConstraintResult;

    /// Adjust a rectangle to satisfy this constraint.
    ///
    /// If check_constraint returned Adjust, this is called to compute
    /// the adjusted rectangle that satisfies the constraint.
    ///
    /// # Arguments
    /// * `window_id` - The window being constrained
    /// * `info` - Information about the requested operation
    ///
    /// # Returns
    /// An adjusted rectangle, or None if no valid adjustment is possible
    fn adjust_rectangle(&self, window_id: WindowId, info: &ConstraintInfo) -> Option<Rectangle> {
        // Default: no adjustment
        let _ = (window_id, info);
        None
    }
}

/// A collection of external constraints.
pub struct ConstraintSet {
    constraints: alloc::vec::Vec<alloc::boxed::Box<dyn ExternalConstraint>>,
}

impl ConstraintSet {
    /// Create a new empty constraint set.
    pub fn new() -> Self {
        ConstraintSet {
            constraints: alloc::vec::Vec::new(),
        }
    }

    /// Add a constraint to this set.
    pub fn add_constraint(&mut self, constraint: alloc::boxed::Box<dyn ExternalConstraint>) {
        self.constraints.push(constraint);
    }

    /// Check if all constraints are satisfied for a window operation.
    ///
    /// Returns the first constraint result that is not Allow.
    pub fn check_all(&self, window_id: WindowId, info: &ConstraintInfo) -> ConstraintResult {
        for constraint in &self.constraints {
            let result = constraint.check_constraint(window_id, info);
            if result != ConstraintResult::Allow {
                return result;
            }
        }
        ConstraintResult::Allow
    }

    /// Try to adjust a rectangle using all constraints.
    ///
    /// Iterates through constraints and applies adjustments until satisfied.
    pub fn adjust_all(&self, window_id: WindowId, info: &ConstraintInfo) -> Option<Rectangle> {
        let mut rect = info.new_rect;

        for constraint in &self.constraints {
            let result = constraint.check_constraint(window_id, info);
            if result == ConstraintResult::Adjust {
                if let Some(adjusted) = constraint.adjust_rectangle(window_id, info) {
                    rect = adjusted;
                } else {
                    return None;
                }
            } else if result == ConstraintResult::Deny {
                return None;
            }
        }

        Some(rect)
    }
}

impl Default for ConstraintSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectangle_creation() {
        let rect = Rectangle::new(10, 20, 100, 50);
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 20);
        assert_eq!(rect.width, 100);
        assert_eq!(rect.height, 50);
    }

    #[test]
    fn test_rectangle_area() {
        let rect = Rectangle::new(0, 0, 100, 50);
        assert_eq!(rect.area(), 5000);
    }

    #[test]
    fn test_rectangle_intersects() {
        let rect1 = Rectangle::new(0, 0, 100, 100);
        let rect2 = Rectangle::new(50, 50, 100, 100);
        assert!(rect1.intersects(&rect2));
    }

    #[test]
    fn test_rectangle_no_intersect() {
        let rect1 = Rectangle::new(0, 0, 100, 100);
        let rect2 = Rectangle::new(200, 200, 100, 100);
        assert!(!rect1.intersects(&rect2));
    }

    #[test]
    fn test_constraint_set() {
        let set = ConstraintSet::new();
        let info = ConstraintInfo::new(
            Rectangle::new(0, 0, 100, 100),
            Rectangle::new(0, 0, 100, 100),
            false,
            false,
        );
        assert_eq!(set.check_all(WindowId(1), &info), ConstraintResult::Allow);
    }
}
