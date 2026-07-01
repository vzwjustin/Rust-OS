//! Bezier curve utilities for animation and interpolation.
//!
//! Provides cubic Bezier curve evaluation used for smooth animations.
//! Ported from `meta-bezier.c`.

/// Cubic Bezier curve control points
#[derive(Debug, Clone, Copy)]
pub struct BezierCurve {
    /// First control point x
    pub p1_x: f32,
    /// First control point y
    pub p1_y: f32,
    /// Second control point x
    pub p2_x: f32,
    /// Second control point y
    pub p2_y: f32,
}

impl BezierCurve {
    /// Create a new Bezier curve from control points
    /// Points should be between 0 and 1 for typical easing curves
    pub fn new(p1_x: f32, p1_y: f32, p2_x: f32, p2_y: f32) -> Self {
        BezierCurve {
            p1_x,
            p1_y,
            p2_x,
            p2_y,
        }
    }

    /// Linear interpolation helper
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a * (1.0 - t) + b * t
    }

    /// Evaluate the Bezier curve at parameter t (0..1)
    pub fn eval(&self, t: f32) -> f32 {
        // Clamp t to valid range
        let t = t.max(0.0).min(1.0);

        // De Casteljau's algorithm for cubic Bezier
        // Start points: (0,0), (p1_x, p1_y), (p2_x, p2_y), (1,1)
        let p0_x = 0.0;
        let p0_y = 0.0;
        let p3_x = 1.0;
        let p3_y = 1.0;

        // First level
        let q0_x = Self::lerp(p0_x, self.p1_x, t);
        let q0_y = Self::lerp(p0_y, self.p1_y, t);
        let q1_x = Self::lerp(self.p1_x, self.p2_x, t);
        let q1_y = Self::lerp(self.p1_y, self.p2_y, t);
        let q2_x = Self::lerp(self.p2_x, p3_x, t);
        let q2_y = Self::lerp(self.p2_y, p3_y, t);

        // Second level
        let r0_x = Self::lerp(q0_x, q1_x, t);
        let r0_y = Self::lerp(q0_y, q1_y, t);
        let r1_x = Self::lerp(q1_x, q2_x, t);
        let r1_y = Self::lerp(q1_y, q2_y, t);

        // Final point
        let s_x = Self::lerp(r0_x, r1_x, t);
        let s_y = Self::lerp(r0_y, r1_y, t);

        s_y
    }

    /// Standard ease-in easing function
    pub fn ease_in(t: f32) -> f32 {
        let curve = BezierCurve::new(0.42, 0.0, 1.0, 1.0);
        curve.eval(t)
    }

    /// Standard ease-out easing function
    pub fn ease_out(t: f32) -> f32 {
        let curve = BezierCurve::new(0.0, 0.0, 0.58, 1.0);
        curve.eval(t)
    }

    /// Standard ease-in-out easing function
    pub fn ease_in_out(t: f32) -> f32 {
        let curve = BezierCurve::new(0.42, 0.0, 0.58, 1.0);
        curve.eval(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bezier_creation() {
        let curve = BezierCurve::new(0.25, 0.1, 0.25, 1.0);
        assert_eq!(curve.p1_x, 0.25);
        assert_eq!(curve.p1_y, 0.1);
    }

    #[test]
    fn test_bezier_endpoints() {
        let curve = BezierCurve::new(0.25, 0.1, 0.25, 1.0);
        // At t=0, should be at (0, 0)
        let y0 = curve.eval(0.0);
        assert!(y0.abs() < 0.01);
        // At t=1, should be at (1, 1)
        let y1 = curve.eval(1.0);
        assert!((y1 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_ease_in() {
        let y_mid = BezierCurve::ease_in(0.5);
        // Ease-in should be less than 0.5 at t=0.5
        assert!(y_mid < 0.5);
    }

    #[test]
    fn test_ease_out() {
        let y_mid = BezierCurve::ease_out(0.5);
        // Ease-out should be greater than 0.5 at t=0.5
        assert!(y_mid > 0.5);
    }
}
