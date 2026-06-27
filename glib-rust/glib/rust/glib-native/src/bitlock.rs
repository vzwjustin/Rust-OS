//! Bit-level locking primitives matching `gbitlock.h` / `gbitlock.c`.
//!
//! Provides `g_bit_lock` / `g_bit_unlock` for locking a specific bit in an
//! atomic integer, and `g_pointer_bit_lock` for pointer-sized bit locks.
//! All operations use `core::sync::atomic` and are fully `no_std` compatible.

use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

/// Spin until `lock_bit` of `*address` is atomically set.
///
/// `address` must be aligned to 4 bytes. `lock_bit` must be in `0..31`.
pub fn bit_lock(address: &AtomicI32, lock_bit: u32) {
    let mask = 1i32 << lock_bit;
    loop {
        let old = address.fetch_or(mask, Ordering::Acquire);
        if (old & mask) == 0 {
            return;
        }
        // Spin-wait: try again
        while (address.load(Ordering::Relaxed) & mask) != 0 {
            core::hint::spin_loop();
        }
    }
}

/// Try to set `lock_bit` of `*address` once. Returns `true` on success.
pub fn bit_trylock(address: &AtomicI32, lock_bit: u32) -> bool {
    let mask = 1i32 << lock_bit;
    let old = address.fetch_or(mask, Ordering::Acquire);
    (old & mask) == 0
}

/// Clear `lock_bit` of `*address`.
pub fn bit_unlock(address: &AtomicI32, lock_bit: u32) {
    let mask = 1i32 << lock_bit;
    address.fetch_and(!mask, Ordering::Release);
}

/// Lock `lock_bit` of a pointer-sized `address` (usize).
pub fn pointer_bit_lock(address: &AtomicUsize, lock_bit: u32) {
    let mask = 1usize << lock_bit;
    loop {
        let old = address.fetch_or(mask, Ordering::Acquire);
        if (old & mask) == 0 {
            return;
        }
        while (address.load(Ordering::Relaxed) & mask) != 0 {
            core::hint::spin_loop();
        }
    }
}

/// Try to lock `lock_bit` of a pointer-sized `address` once.
pub fn pointer_bit_trylock(address: &AtomicUsize, lock_bit: u32) -> bool {
    let mask = 1usize << lock_bit;
    let old = address.fetch_or(mask, Ordering::Acquire);
    (old & mask) == 0
}

/// Unlock `lock_bit` of a pointer-sized `address`.
pub fn pointer_bit_unlock(address: &AtomicUsize, lock_bit: u32) {
    let mask = 1usize << lock_bit;
    address.fetch_and(!mask, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_lock_unlock() {
        let val = AtomicI32::new(0);
        assert!(bit_trylock(&val, 0));
        assert!(!bit_trylock(&val, 0));
        bit_unlock(&val, 0);
        assert!(bit_trylock(&val, 0));
        bit_unlock(&val, 0);
    }

    #[test]
    fn bit_lock_different_bits() {
        let val = AtomicI32::new(0);
        assert!(bit_trylock(&val, 0));
        assert!(bit_trylock(&val, 1));
        assert!(!bit_trylock(&val, 0));
        assert!(!bit_trylock(&val, 1));
        bit_unlock(&val, 0);
        assert!(bit_trylock(&val, 0));
        bit_unlock(&val, 0);
        bit_unlock(&val, 1);
    }

    #[test]
    fn pointer_bit_lock_unlock() {
        let val = AtomicUsize::new(0);
        assert!(pointer_bit_trylock(&val, 0));
        assert!(!pointer_bit_trylock(&val, 0));
        pointer_bit_unlock(&val, 0);
        assert!(pointer_bit_trylock(&val, 0));
        pointer_bit_unlock(&val, 0);
    }
}
