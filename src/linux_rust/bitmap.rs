//! Bitmap operations.
//!
//! Ported from Linux `rust/kernel/bitmap.rs`, reimplemented in pure Rust
//! without C binding dependencies.
//!
//! C headers: `include/linux/bitmap.h`

use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

/// An owned bitmap with a pure-Rust backing store.
///
/// Provides `set_bit`, `clear_bit`, `next_bit`, `next_zero_bit`, `last_bit`
/// operations matching the Linux kernel `Bitmap` API.
pub struct BitmapVec {
    data: Vec<usize>,
    nbits: usize,
}

/// Number of bits per `usize` element.
const BITS_PER_USIZE: usize = usize::BITS as usize;

impl BitmapVec {
    /// The maximum possible length of a `BitmapVec`.
    pub const MAX_LEN: usize = i32::MAX as usize;

    /// Construct a new zero-initialized `BitmapVec` with `nbits` bits.
    pub fn new(nbits: usize) -> Self {
        let words = nbits.div_ceil(BITS_PER_USIZE);
        BitmapVec {
            data: vec![0usize; words],
            nbits,
        }
    }

    /// Returns the number of bits in this bitmap.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.nbits
    }

    /// Set bit at `index`.
    #[inline]
    pub fn set_bit(&mut self, index: usize) {
        if index >= self.nbits {
            return;
        }
        let word = index / BITS_PER_USIZE;
        let bit = index % BITS_PER_USIZE;
        self.data[word] |= 1 << bit;
    }

    /// Set bit at `index`, atomically (relaxed ordering).
    #[inline]
    pub fn set_bit_atomic(&self, index: usize) {
        if index >= self.nbits {
            return;
        }
        let word = index / BITS_PER_USIZE;
        let bit = index % BITS_PER_USIZE;
        // SAFETY: `word` is within bounds (checked above), and we only use
        // the reference for an atomic RMW which is safe for concurrent access.
        unsafe {
            let ptr = self.data.as_ptr().add(word) as *const AtomicUsize;
            (*ptr).fetch_or(1 << bit, Ordering::Relaxed);
        }
    }

    /// Clear bit at `index`.
    #[inline]
    pub fn clear_bit(&mut self, index: usize) {
        if index >= self.nbits {
            return;
        }
        let word = index / BITS_PER_USIZE;
        let bit = index % BITS_PER_USIZE;
        self.data[word] &= !(1 << bit);
    }

    /// Clear bit at `index`, atomically (relaxed ordering).
    #[inline]
    pub fn clear_bit_atomic(&self, index: usize) {
        if index >= self.nbits {
            return;
        }
        let word = index / BITS_PER_USIZE;
        let bit = index % BITS_PER_USIZE;
        // SAFETY: `word` is within bounds (checked above), and we only use
        // the reference for an atomic RMW which is safe for concurrent access.
        unsafe {
            let ptr = self.data.as_ptr().add(word) as *const AtomicUsize;
            (*ptr).fetch_and(!(1 << bit), Ordering::Relaxed);
        }
    }

    /// Test if bit at `index` is set.
    #[inline]
    pub fn test_bit(&self, index: usize) -> bool {
        if index >= self.nbits {
            return false;
        }
        let word = index / BITS_PER_USIZE;
        let bit = index % BITS_PER_USIZE;
        (self.data[word] & (1 << bit)) != 0
    }

    /// Find next set bit, starting from `start`.
    /// Returns `None` if no set bit is found.
    #[inline]
    pub fn next_bit(&self, start: usize) -> Option<usize> {
        if start >= self.nbits {
            return None;
        }
        let mut word_idx = start / BITS_PER_USIZE;
        let bit_idx = start % BITS_PER_USIZE;

        // Check first word with mask
        let mask = !((1 << bit_idx) - 1);
        let mut word = self.data[word_idx] & mask;
        loop {
            if word != 0 {
                let bit = word.trailing_zeros() as usize;
                let result = word_idx * BITS_PER_USIZE + bit;
                if result < self.nbits {
                    return Some(result);
                }
                return None;
            }
            word_idx += 1;
            if word_idx >= self.data.len() {
                return None;
            }
            word = self.data[word_idx];
        }
    }

    /// Find next zero bit, starting from `start`.
    /// Returns `None` if no zero bit is found.
    #[inline]
    pub fn next_zero_bit(&self, start: usize) -> Option<usize> {
        if start >= self.nbits {
            return None;
        }
        let mut word_idx = start / BITS_PER_USIZE;
        let bit_idx = start % BITS_PER_USIZE;

        let mask = !((1 << bit_idx) - 1);
        let mut word = (!self.data[word_idx]) & mask;
        loop {
            if word != 0 {
                let bit = word.trailing_zeros() as usize;
                let result = word_idx * BITS_PER_USIZE + bit;
                if result < self.nbits {
                    return Some(result);
                }
                return None;
            }
            word_idx += 1;
            if word_idx >= self.data.len() {
                return None;
            }
            word = !self.data[word_idx];
        }
    }

    /// Find last set bit.
    /// Returns `None` if all bits are zero.
    #[inline]
    pub fn last_bit(&self) -> Option<usize> {
        for (word_idx, &word) in self.data.iter().enumerate().rev() {
            if word != 0 {
                let bit = BITS_PER_USIZE - 1 - word.leading_zeros() as usize;
                let result = word_idx * BITS_PER_USIZE + bit;
                if result < self.nbits {
                    return Some(result);
                }
            }
        }
        None
    }

    /// Copy `src` into this bitmap and zero any remaining bits.
    pub fn copy_and_extend(&mut self, src: &BitmapVec) {
        let len = core::cmp::min(src.nbits, self.nbits);
        let src_words = len.div_ceil(BITS_PER_USIZE);
        for i in 0..src_words {
            self.data[i] = src.data.get(i).copied().unwrap_or(0);
        }
        // Zero remaining words
        for i in src_words..self.data.len() {
            self.data[i] = 0;
        }
        // Mask off bits beyond nbits in the last word
        let last_word_bits = self.nbits % BITS_PER_USIZE;
        if last_word_bits != 0 && !self.data.is_empty() {
            let last_idx = self.data.len() - 1;
            let mask = (1 << last_word_bits) - 1;
            self.data[last_idx] &= mask;
        }
    }

    /// Clear all bits.
    pub fn clear(&mut self) {
        for word in &mut self.data {
            *word = 0;
        }
    }

    /// Fill all bits with 1.
    pub fn fill(&mut self) {
        for word in &mut self.data {
            *word = usize::MAX;
        }
        // Mask off bits beyond nbits in the last word
        let last_word_bits = self.nbits % BITS_PER_USIZE;
        if last_word_bits != 0 && !self.data.is_empty() {
            let last_idx = self.data.len() - 1;
            let mask = (1 << last_word_bits) - 1;
            self.data[last_idx] &= mask;
        }
    }

    /// Count the number of set bits.
    pub fn weight(&self) -> usize {
        self.data.iter().map(|&w| w.count_ones() as usize).sum()
    }
}

impl core::fmt::Debug for BitmapVec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BitmapVec")
            .field("nbits", &self.nbits)
            .field("weight", &self.weight())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_bitmap_basic() {
        let mut b = BitmapVec::new(128);
        assert_eq!(b.len(), 128);
        assert_eq!(b.next_bit(0), None);
        assert_eq!(b.next_zero_bit(0), Some(0));
        assert_eq!(b.last_bit(), None);

        b.set_bit(17);
        assert_eq!(b.next_bit(0), Some(17));
        assert_eq!(b.next_bit(17), Some(17));
        assert_eq!(b.next_bit(18), None);
        assert_eq!(b.last_bit(), Some(17));

        b.set_bit(107);
        assert_eq!(b.next_bit(0), Some(17));
        assert_eq!(b.next_bit(18), Some(107));
        assert_eq!(b.last_bit(), Some(107));

        b.clear_bit(17);
        assert_eq!(b.next_bit(0), Some(107));
        assert_eq!(b.last_bit(), Some(107));
    }

    #[test_case]
    fn test_bitmap_copy_and_extend() {
        let mut long_bitmap = BitmapVec::new(256);
        long_bitmap.set_bit(3);
        long_bitmap.set_bit(200);

        let mut short_bitmap = BitmapVec::new(32);
        short_bitmap.set_bit(17);

        long_bitmap.copy_and_extend(&short_bitmap);
        assert_eq!(long_bitmap.next_bit(0), Some(17));
        assert_eq!(long_bitmap.last_bit(), Some(17));
    }

    #[test_case]
    fn test_bitmap_weight() {
        let mut b = BitmapVec::new(64);
        b.set_bit(0);
        b.set_bit(31);
        b.set_bit(63);
        assert_eq!(b.weight(), 3);
    }
}
