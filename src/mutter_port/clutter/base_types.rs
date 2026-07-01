//! Port of GNOME mutter's `clutter/clutter/clutter-base-types.{c,h}`.
//!
//! `ClutterMargin` represents the components of a margin (left, right, top, bottom).
//! This port drops GObject reference-counted allocation glue; `Margin` is a plain
//! `Copy` struct in Rust.

/// A representation of the components of a margin.
///
/// Mirrors `ClutterMargin` from mutter's `clutter` library.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Margin {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Margin {
    /// Creates a new `Margin` with all fields set to zero.
    ///
    /// Mirrors `clutter_margin_new`.
    pub fn new() -> Self {
        Margin {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        }
    }

    /// Creates a new `Margin` with the given values.
    pub fn with_values(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Margin {
            left,
            right,
            top,
            bottom,
        }
    }

    /// Returns the total horizontal margin (left + right).
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    /// Returns the total vertical margin (top + bottom).
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }

    /// Returns whether all margins are zero.
    pub fn is_zero(&self) -> bool {
        self.left == 0.0 && self.right == 0.0 && self.top == 0.0 && self.bottom == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = Margin::new();
        assert_eq!(m.left, 0.0);
        assert_eq!(m.right, 0.0);
        assert_eq!(m.top, 0.0);
        assert_eq!(m.bottom, 0.0);
    }

    #[test]
    fn test_with_values() {
        let m = Margin::with_values(1.0, 2.0, 3.0, 4.0);
        assert_eq!(m.left, 1.0);
        assert_eq!(m.right, 2.0);
        assert_eq!(m.top, 3.0);
        assert_eq!(m.bottom, 4.0);
    }

    #[test]
    fn test_horizontal() {
        let m = Margin::with_values(5.0, 10.0, 2.0, 3.0);
        assert_eq!(m.horizontal(), 15.0);
    }

    #[test]
    fn test_vertical() {
        let m = Margin::with_values(5.0, 10.0, 2.0, 3.0);
        assert_eq!(m.vertical(), 5.0);
    }

    #[test]
    fn test_is_zero() {
        let zero = Margin::new();
        assert!(zero.is_zero());

        let nonzero = Margin::with_values(1.0, 0.0, 0.0, 0.0);
        assert!(!nonzero.is_zero());
    }
}
