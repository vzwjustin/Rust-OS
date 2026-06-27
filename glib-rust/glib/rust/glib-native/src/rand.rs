//! Random number generator matching `grand.h` / `grand.c`.
//!
//! Implements the Mersenne Twister (MT19937) algorithm, same as GLib.
//! Fully `no_std` compatible.

/// Mersenne Twister state size.
const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// A random number generator (`GRand`).
///
/// Implements the Mersenne Twister algorithm (MT19937).
pub struct Rand {
    mt: [u32; N],
    index: usize,
}

impl Rand {
    /// Create a new RNG with a seed (`g_rand_new_with_seed`).
    pub fn with_seed(seed: u32) -> Self {
        let mut rng = Self {
            mt: [0u32; N],
            index: N,
        };
        rng.set_seed(seed);
        rng
    }

    /// Create a new RNG with a seed array (`g_rand_new_with_seed_array`).
    pub fn with_seed_array(seeds: &[u32]) -> Self {
        let mut rng = Self {
            mt: [0u32; N],
            index: N,
        };
        rng.set_seed_array(seeds);
        rng
    }

    /// Create a new RNG with a default seed (`g_rand_new`).
    ///
    /// In no_std, we use a fixed seed since we don't have /dev/urandom.
    pub fn new() -> Self {
        Self::with_seed(0x6d4f_3a2b)
    }

    /// Set the seed (`g_rand_set_seed`).
    pub fn set_seed(&mut self, seed: u32) {
        self.mt[0] = seed;
        for i in 1..N {
            self.mt[i] = 1812433253u32
                .wrapping_mul(self.mt[i - 1] ^ (self.mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        self.index = N;
    }

    /// Set the seed from an array (`g_rand_set_seed_array`).
    pub fn set_seed_array(&mut self, seeds: &[u32]) {
        self.set_seed(19650218);
        let mut i = 1usize;
        let mut j = 0usize;
        let k = if N > seeds.len() { N } else { seeds.len() };
        for _ in 0..k {
            self.mt[i] = (self.mt[i]
                ^ ((self.mt[i - 1] ^ (self.mt[i - 1] >> 30)) * 1664525))
            .wrapping_add(seeds[j]);
            i += 1;
            j += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            if j >= seeds.len() {
                j = 0;
            }
        }
        for _ in 1..N {
            self.mt[i] = self.mt[i]
                ^ ((self.mt[i - 1] ^ (self.mt[i - 1] >> 30)) * 1566083941)
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
        }
        self.mt[0] = 0x8000_0000;
        self.index = N;
    }

    /// Copy the RNG (`g_rand_copy`).
    pub fn copy(&self) -> Self {
        Self {
            mt: self.mt,
            index: self.index,
        }
    }

    /// Generate the next 32-bit random number (`g_rand_int`).
    pub fn int(&mut self) -> u32 {
        if self.index >= N {
            self.generate_numbers();
        }

        let mut y = self.mt[self.index];
        self.index += 1;

        // Tempering
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;

        y
    }

    /// Generate a random boolean (`g_rand_boolean`).
    pub fn boolean(&mut self) -> bool {
        (self.int() & (1 << 15)) != 0
    }

    /// Generate a random integer in [begin, end) (`g_rand_int_range`).
    pub fn int_range(&mut self, begin: i32, end: i32) -> i32 {
        if end <= begin {
            return begin;
        }
        let range = (end - begin) as u32;
        begin + (self.int() % range) as i32
    }

    /// Generate a random double in [0, 1) (`g_rand_double`).
    pub fn double(&mut self) -> f64 {
        (self.int() as f64) / (u32::MAX as f64 + 1.0)
    }

    /// Generate a random double in [begin, end) (`g_rand_double_range`).
    pub fn double_range(&mut self, begin: f64, end: f64) -> f64 {
        begin + self.double() * (end - begin)
    }

    fn generate_numbers(&mut self) {
        for i in 0..(N - M) {
            let y = (self.mt[i] & UPPER_MASK) | (self.mt[i + 1] & LOWER_MASK);
            self.mt[i] = self.mt[i + M] ^ (y >> 1) ^ if y & 1 != 0 { MATRIX_A } else { 0 };
        }
        for i in (N - M)..(N - 1) {
            let y = (self.mt[i] & UPPER_MASK) | (self.mt[i + 1] & LOWER_MASK);
            self.mt[i] = self.mt[i - (N - M)] ^ (y >> 1) ^ if y & 1 != 0 { MATRIX_A } else { 0 };
        }
        let y = (self.mt[N - 1] & UPPER_MASK) | (self.mt[0] & LOWER_MASK);
        self.mt[N - 1] = self.mt[M - 1] ^ (y >> 1) ^ if y & 1 != 0 { MATRIX_A } else { 0 };
        self.index = 0;
    }
}

impl Clone for Rand {
    fn clone(&self) -> Self {
        self.copy()
    }
}

impl Default for Rand {
    fn default() -> Self {
        Self::new()
    }
}

/// Global random number generator for `g_random_*` functions.
static GLOBAL_RAND: spin::Mutex<Rand> = spin::Mutex::new(Rand {
    mt: [0x6d4f_3a2b; N],
    index: N,
});

/// Set the global RNG seed (`g_random_set_seed`).
pub fn random_set_seed(seed: u32) {
    GLOBAL_RAND.lock().set_seed(seed);
}

/// Generate a random 32-bit integer from the global RNG (`g_random_int`).
pub fn random_int() -> u32 {
    GLOBAL_RAND.lock().int()
}

/// Generate a random boolean from the global RNG (`g_random_boolean`).
pub fn random_boolean() -> bool {
    (random_int() & (1 << 15)) != 0
}

/// Generate a random integer in [begin, end) from the global RNG (`g_random_int_range`).
pub fn random_int_range(begin: i32, end: i32) -> i32 {
    GLOBAL_RAND.lock().int_range(begin, end)
}

/// Generate a random double in [0, 1) from the global RNG (`g_random_double`).
pub fn random_double() -> f64 {
    GLOBAL_RAND.lock().double()
}

/// Generate a random double in [begin, end) from the global RNG (`g_random_double_range`).
pub fn random_double_range(begin: f64, end: f64) -> f64 {
    GLOBAL_RAND.lock().double_range(begin, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_reproducible() {
        let mut a = Rand::with_seed(42);
        let mut b = Rand::with_seed(42);
        for _ in 0..100 {
            assert_eq!(a.int(), b.int());
        }
    }

    #[test]
    fn int_range() {
        let mut r = Rand::with_seed(123);
        for _ in 0..1000 {
            let v = r.int_range(10, 20);
            assert!(v >= 10 && v < 20);
        }
    }

    #[test]
    fn double_range() {
        let mut r = Rand::with_seed(456);
        for _ in 0..1000 {
            let v = r.double();
            assert!(v >= 0.0 && v < 1.0);
        }
    }

    #[test]
    fn double_range_custom() {
        let mut r = Rand::with_seed(789);
        for _ in 0..1000 {
            let v = r.double_range(5.0, 10.0);
            assert!(v >= 5.0 && v < 10.0);
        }
    }

    #[test]
    fn boolean() {
        let mut r = Rand::with_seed(999);
        let mut true_count = 0;
        for _ in 0..1000 {
            if r.boolean() {
                true_count += 1;
            }
        }
        // Should be roughly 50/50
        assert!(true_count > 300 && true_count < 700);
    }

    #[test]
    fn copy() {
        let mut a = Rand::with_seed(42);
        let mut b = a.copy();
        assert_eq!(a.int(), b.int());
    }

    #[test]
    fn global_random() {
        random_set_seed(42);
        let v = random_int();
        random_set_seed(42);
        assert_eq!(random_int(), v);
    }

    #[test]
    fn seed_array() {
        let mut a = Rand::with_seed_array(&[1, 2, 3, 4, 5]);
        let mut b = Rand::with_seed_array(&[1, 2, 3, 4, 5]);
        assert_eq!(a.int(), b.int());
    }
}
