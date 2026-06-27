//! Prime numbers matching `gprimes.h` / `gprimes.c`.
//!
//! Provides `spaced_primes_closest` for resizing hash tables.
//! Fully `no_std` compatible.

/// The precomputed table of primes from GLib, spaced approximately 1.5-2x.
const PRIMES: [u32; 71] = [
    11, 13, 17, 23, 29, 37, 47, 59, 71, 89, 107, 131, 163, 197, 239, 293, 353,
    431, 521, 631, 761, 919, 1103, 1327, 1597, 1931, 2333, 2801, 3371, 4049,
    4861, 5839, 7013, 8419, 10103, 12143, 14591, 17519, 21023, 25229, 30293,
    36353, 43627, 52361, 62851, 75431, 90523, 108631, 130363, 156437, 187751,
    225307, 270371, 324449, 389357, 467237, 560689, 672827, 807403, 968897,
    1162687, 1395263, 1674319, 2009191, 2411033, 2893249, 3471899, 4166287,
    4999559, 5999471, 7199369,
];

/// Returns the closest prime number >= `num` (`g_spaced_primes_closest`).
///
/// Used for resizing hash tables to prime-valued sizes.
pub fn spaced_primes_closest(num: u32) -> u32 {
    for &p in PRIMES.iter() {
        if p >= num {
            return p;
        }
    }
    *PRIMES.last().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_numbers() {
        assert_eq!(spaced_primes_closest(0), 11);
        assert_eq!(spaced_primes_closest(1), 11);
        assert_eq!(spaced_primes_closest(11), 11);
        assert_eq!(spaced_primes_closest(12), 13);
    }

    #[test]
    fn medium_numbers() {
        assert_eq!(spaced_primes_closest(100), 107);
        assert_eq!(spaced_primes_closest(108), 131);
    }

    #[test]
    fn large_numbers() {
        assert!(spaced_primes_closest(1000) >= 1000);
        assert!(spaced_primes_closest(10000) >= 10000);
    }

    #[test]
    fn returns_prime() {
        let p = spaced_primes_closest(500);
        assert!(is_prime(p));
    }

    fn is_prime(n: u32) -> bool {
        if n < 2 { return false; }
        if n < 4 { return true; }
        if n % 2 == 0 { return false; }
        let mut i = 3;
        while i * i <= n {
            if n % i == 0 { return false; }
            i += 2;
        }
        true
    }
}
