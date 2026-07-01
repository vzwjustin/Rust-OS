//! Fraction utility ported from GNOME Mutter (src/core/meta-fraction.c).
//!
//! Provides utilities for converting floating-point numbers to rational fractions.
//! Uses continued fraction approximation algorithm from GStreamer/gstutils.c.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-fraction.c

/// Represents a rational number as numerator/denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fraction {
    /// Numerator
    pub num: i32,
    /// Denominator
    pub denom: i32,
}

impl Fraction {
    /// Create a new fraction from numerator and denominator.
    pub fn new(num: i32, denom: i32) -> Self {
        Fraction { num, denom }
    }

    /// Convert a floating-point number to a fraction using continued fraction approximation.
    ///
    /// This algorithm approximates a double as a rational number with bounded error.
    /// It handles negative numbers and guards against overflow.
    ///
    /// # Arguments
    /// * `src` - The floating-point value to convert
    ///
    /// # Returns
    /// A Fraction that closely approximates the input value
    pub fn from_f64(src: f64) -> Self {
        const MAX_TERMS: usize = 30;
        const MIN_DIVISOR: f64 = 1.0e-10;
        const MAX_ERROR: f64 = 1.0e-20;

        let mut v = src;
        let mut f = src;
        let negative = f < 0.0;

        if negative {
            f = -f;
        }

        // Initialize fractions with 1/0, 0/1
        let mut n1: i64 = 1;
        let mut d1: i64 = 0;
        let mut n2: i64 = 0;
        let mut d2: i64 = 1;
        let mut n: i32 = 1;
        let mut d: i32 = 1;

        for _ in 0..MAX_TERMS {
            let a = f as i32; // No floor() needed; f is always >= 0
            f = f - a as f64;

            // Calculate new fraction
            n2 = n1 * a as i64 + n2;
            d2 = d1 * a as i64 + d2;

            // Guard against overflow
            if n2 > i32::MAX as i64 || d2 > i32::MAX as i64 {
                break;
            }

            n = n2 as i32;
            d = d2 as i32;

            // Save last two fractions
            n2 = n1;
            d2 = d1;
            n1 = n as i64;
            d1 = d as i64;

            // Quit if dividing by zero or close enough to target
            if f < MIN_DIVISOR || (v - (n as f64 / d as f64)).abs() < MAX_ERROR {
                break;
            }

            // Take reciprocal
            f = 1.0 / f;
        }

        // Fix for overflow
        if d == 0 {
            n = i32::MAX;
            d = 1;
        }

        // Fix for negative
        if negative {
            n = -n;
        }

        // Simplify by GCD
        let gcd = Self::gcd(n, d);
        if gcd != 0 {
            n /= gcd;
            d /= gcd;
        }

        Fraction { num: n, denom: d }
    }

    /// Calculate the greatest common divisor of two integers.
    fn gcd(mut a: i32, mut b: i32) -> i32 {
        while b != 0 {
            let temp = a;
            a = b;
            b = temp % b;
        }
        a.abs()
    }

    /// Get the decimal value of this fraction.
    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.denom as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fraction_creation() {
        let frac = Fraction::new(1, 2);
        assert_eq!(frac.num, 1);
        assert_eq!(frac.denom, 2);
    }

    #[test]
    fn test_fraction_from_f64_simple() {
        let frac = Fraction::from_f64(0.5);
        assert_eq!(frac.num, 1);
        assert_eq!(frac.denom, 2);
    }

    #[test]
    fn test_fraction_from_f64_third() {
        let frac = Fraction::from_f64(1.0 / 3.0);
        assert_eq!(frac.num, 1);
        assert_eq!(frac.denom, 3);
    }

    #[test]
    fn test_fraction_from_f64_negative() {
        let frac = Fraction::from_f64(-0.5);
        assert_eq!(frac.num, -1);
        assert_eq!(frac.denom, 2);
    }

    #[test]
    fn test_fraction_from_f64_pi_approximation() {
        let frac = Fraction::from_f64(core::f64::consts::PI);
        // Should be close to 22/7 or similar
        let error = (frac.as_f64() - core::f64::consts::PI).abs();
        assert!(error < 0.01); // Reasonably close approximation
    }

    #[test]
    fn test_fraction_as_f64() {
        let frac = Fraction::new(3, 4);
        assert!((frac.as_f64() - 0.75).abs() < 1e-10);
    }
}
