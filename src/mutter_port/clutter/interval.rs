//! Port of GNOME mutter's `clutter/clutter-interval.{c,h}`.
//!
//! ClutterInterval holds two values defining an animation interval.
//! Interpolates between initial and final values at a given progress.

/// An interval between two f64 values with progress-based interpolation.
#[derive(Debug, Clone, Copy)]
pub struct Interval {
    initial: f64,
    final_value: f64,
}

impl Interval {
    /// Create a new interval between `initial` and `final_value`.
    pub fn new(initial: f64, final_value: f64) -> Self {
        Interval {
            initial,
            final_value,
        }
    }

    /// Get the initial value.
    pub fn initial(&self) -> f64 {
        self.initial
    }

    /// Get the final value.
    pub fn final_value(&self) -> f64 {
        self.final_value
    }

    /// Set the initial value.
    pub fn set_initial(&mut self, initial: f64) {
        self.initial = initial;
    }

    /// Set the final value.
    pub fn set_final(&mut self, final_value: f64) {
        self.final_value = final_value;
    }

    /// Compute the interpolated value at progress `factor` (0.0 to 1.0).
    /// Returns `initial + (final - initial) * factor`.
    pub fn compute_value(&self, factor: f64) -> f64 {
        self.initial + (self.final_value - self.initial) * factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_value_at_start() {
        let interval = Interval::new(0.0, 10.0);
        assert!((interval.compute_value(0.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn compute_value_at_end() {
        let interval = Interval::new(0.0, 10.0);
        assert!((interval.compute_value(1.0) - 10.0).abs() < 1e-10);
    }

    #[test]
    fn compute_value_at_midpoint() {
        let interval = Interval::new(0.0, 10.0);
        assert!((interval.compute_value(0.5) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn compute_value_negative_range() {
        let interval = Interval::new(-10.0, 10.0);
        assert!((interval.compute_value(0.5) - 0.0).abs() < 1e-10);
    }
}
