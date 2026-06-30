// SPDX-License-Identifier: GPL-2.0
//! Bitmap primitives — pure Rust port of Linux kernel bitmap.h
//!
//! Provides:
//! - `Bitmap<const BITS: usize>` — fixed-size bitmap backed by a const array of u64 words
//! - `DynBitmap` — heap-allocated bitmap with runtime size
//! - `CpuMask` — type alias: `Bitmap<256>`

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;
use alloc::vec::Vec;
use core::fmt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const fn words_for(bits: usize) -> usize {
    (bits + 63) / 64
}

// ---------------------------------------------------------------------------
// Fixed-size Bitmap<const BITS: usize>
// ---------------------------------------------------------------------------

/// Fixed-size bitmap backed by `[(BITS+63)/64]` u64 words.
/// All bit indices are zero-based; bits beyond `BITS` are always zero.
#[derive(Clone)]
pub struct Bitmap<const BITS: usize>
where
    [u64; words_for(BITS)]: Sized,
{
    words: [u64; words_for(BITS)],
}

impl<const BITS: usize> Default for Bitmap<BITS>
where
    [u64; words_for(BITS)]: Sized,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const BITS: usize> Bitmap<BITS>
where
    [u64; words_for(BITS)]: Sized,
{
    /// Creates a new, zero-filled bitmap.
    pub const fn new() -> Self {
        Self { words: [0u64; words_for(BITS)] }
    }

    /// Creates a bitmap with all bits set.
    pub fn new_ones() -> Self {
        let mut bm = Self::new();
        for w in bm.words.iter_mut() {
            *w = u64::MAX;
        }
        // Clear any bits beyond BITS in the last word
        if BITS % 64 != 0 {
            let last = words_for(BITS) - 1;
            bm.words[last] = (1u64 << (BITS % 64)) - 1;
        }
        bm
    }

    /// Returns the number of bits this bitmap tracks.
    pub const fn len(&self) -> usize {
        BITS
    }

    #[inline]
    fn check_bit(bit: usize) {
        debug_assert!(bit < BITS, "bit index out of range");
    }

    /// Sets bit `bit`.
    #[inline]
    pub fn set(&mut self, bit: usize) {
        Self::check_bit(bit);
        self.words[bit / 64] |= 1u64 << (bit % 64);
    }

    /// Clears bit `bit`.
    #[inline]
    pub fn clear(&mut self, bit: usize) {
        Self::check_bit(bit);
        self.words[bit / 64] &= !(1u64 << (bit % 64));
    }

    /// Returns `true` if bit `bit` is set.
    #[inline]
    pub fn test(&self, bit: usize) -> bool {
        Self::check_bit(bit);
        (self.words[bit / 64] >> (bit % 64)) & 1 == 1
    }

    /// Atomically sets bit and returns the old value.
    #[inline]
    pub fn test_and_set(&mut self, bit: usize) -> bool {
        let old = self.test(bit);
        self.set(bit);
        old
    }

    /// Atomically clears bit and returns the old value.
    #[inline]
    pub fn test_and_clear(&mut self, bit: usize) -> bool {
        let old = self.test(bit);
        self.clear(bit);
        old
    }

    /// Flips bit `bit`.
    #[inline]
    pub fn flip(&mut self, bit: usize) {
        Self::check_bit(bit);
        self.words[bit / 64] ^= 1u64 << (bit % 64);
    }

    /// Returns the index of the first zero bit, or `None` if all bits are set.
    pub fn find_first_zero(&self) -> Option<usize> {
        self.find_next_zero(0)
    }

    /// Returns the index of the first set bit, or `None` if no bits are set.
    pub fn find_first_set(&self) -> Option<usize> {
        self.find_next_set(0)
    }

    /// Returns the index of the first zero bit at or after `start`, or `None`.
    pub fn find_next_zero(&self, start: usize) -> Option<usize> {
        if start >= BITS { return None; }
        let start_word = start / 64;
        let start_bit = start % 64;

        // Mask out bits before start in the first word
        let first_mask = if start_bit == 0 { u64::MAX } else { u64::MAX << start_bit };
        let first_val = (!self.words[start_word]) & first_mask;

        if first_val != 0 {
            let pos = start_word * 64 + first_val.trailing_zeros() as usize;
            return if pos < BITS { Some(pos) } else { None };
        }

        for w in (start_word + 1)..words_for(BITS) {
            let v = !self.words[w];
            if v != 0 {
                let pos = w * 64 + v.trailing_zeros() as usize;
                return if pos < BITS { Some(pos) } else { None };
            }
        }
        None
    }

    /// Returns the index of the first set bit at or after `start`, or `None`.
    pub fn find_next_set(&self, start: usize) -> Option<usize> {
        if start >= BITS { return None; }
        let start_word = start / 64;
        let start_bit = start % 64;

        let first_mask = if start_bit == 0 { u64::MAX } else { u64::MAX << start_bit };
        let first_val = self.words[start_word] & first_mask;

        if first_val != 0 {
            let pos = start_word * 64 + first_val.trailing_zeros() as usize;
            return if pos < BITS { Some(pos) } else { None };
        }

        for w in (start_word + 1)..words_for(BITS) {
            let v = self.words[w];
            if v != 0 {
                let pos = w * 64 + v.trailing_zeros() as usize;
                return if pos < BITS { Some(pos) } else { None };
            }
        }
        None
    }

    /// Returns the number of set bits.
    pub fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Returns the number of zero bits.
    pub fn count_zeros(&self) -> usize {
        BITS - self.count_ones()
    }

    /// Returns `true` if no bits are set.
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Returns `true` if all `BITS` bits are set.
    pub fn is_full(&self) -> bool {
        self.count_ones() == BITS
    }

    /// Clears all bits.
    pub fn clear_all(&mut self) {
        for w in self.words.iter_mut() {
            *w = 0;
        }
    }

    /// Sets all bits.
    pub fn set_all(&mut self) {
        for w in self.words.iter_mut() {
            *w = u64::MAX;
        }
        // Clamp last word
        if BITS % 64 != 0 {
            let last = words_for(BITS) - 1;
            self.words[last] = (1u64 << (BITS % 64)) - 1;
        }
    }

    /// Bitwise AND with `other` in place.
    pub fn and(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a &= b;
        }
    }

    /// Bitwise OR with `other` in place.
    pub fn or(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a |= b;
        }
    }

    /// Bitwise XOR with `other` in place.
    pub fn xor(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a ^= b;
        }
    }

    /// Bitwise NOT in place (only within `BITS`).
    pub fn not(&mut self) {
        for w in self.words.iter_mut() {
            *w = !*w;
        }
        // Clamp
        if BITS % 64 != 0 {
            let last = words_for(BITS) - 1;
            self.words[last] &= (1u64 << (BITS % 64)) - 1;
        }
    }

    /// Returns an iterator over the indices of all set bits.
    pub fn iter_set(&self) -> BitIterator<'_, BITS>
    where
        [u64; words_for(BITS)]: Sized,
    {
        BitIterator { bm: self, next: 0 }
    }

    /// Returns the underlying word slice.
    pub fn as_words(&self) -> &[u64] {
        &self.words
    }

    /// Returns the underlying word slice mutably.
    pub fn as_words_mut(&mut self) -> &mut [u64] {
        &mut self.words
    }
}

/// Iterator over set bit indices in a `Bitmap<BITS>`.
pub struct BitIterator<'a, const BITS: usize>
where
    [u64; words_for(BITS)]: Sized,
{
    bm: &'a Bitmap<BITS>,
    next: usize,
}

impl<'a, const BITS: usize> Iterator for BitIterator<'a, BITS>
where
    [u64; words_for(BITS)]: Sized,
{
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        let bit = self.bm.find_next_set(self.next)?;
        self.next = bit + 1;
        Some(bit)
    }
}

impl<const BITS: usize> fmt::Debug for Bitmap<BITS>
where
    [u64; words_for(BITS)]: Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bitmap<{}>[", BITS)?;
        for bit in 0..BITS {
            if self.test(bit) {
                write!(f, "{},", bit)?;
            }
        }
        write!(f, "]")
    }
}

// ---------------------------------------------------------------------------
// CpuMask
// ---------------------------------------------------------------------------

/// CPU affinity mask for up to 256 CPUs (matches Linux NR_CPUS default).
pub type CpuMask = Bitmap<256>;

// ---------------------------------------------------------------------------
// DynBitmap — heap-allocated
// ---------------------------------------------------------------------------

/// Heap-allocated bitmap with a runtime-determined size.
pub struct DynBitmap {
    words: Vec<u64>,
    nbits: usize,
}

impl DynBitmap {
    /// Creates a new zero-filled bitmap that can hold `nbits` bits.
    pub fn new(nbits: usize) -> Self {
        Self {
            words: alloc::vec![0u64; words_for(nbits)],
            nbits,
        }
    }

    /// Returns the number of bits this bitmap tracks.
    pub fn len(&self) -> usize {
        self.nbits
    }

    pub fn is_empty_bm(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    #[inline]
    fn check_bit(&self, bit: usize) {
        debug_assert!(bit < self.nbits, "bit index out of range");
    }

    pub fn set(&mut self, bit: usize) {
        self.check_bit(bit);
        self.words[bit / 64] |= 1u64 << (bit % 64);
    }

    pub fn clear(&mut self, bit: usize) {
        self.check_bit(bit);
        self.words[bit / 64] &= !(1u64 << (bit % 64));
    }

    pub fn test(&self, bit: usize) -> bool {
        self.check_bit(bit);
        (self.words[bit / 64] >> (bit % 64)) & 1 == 1
    }

    pub fn flip(&mut self, bit: usize) {
        self.check_bit(bit);
        self.words[bit / 64] ^= 1u64 << (bit % 64);
    }

    pub fn find_next_zero(&self, start: usize) -> Option<usize> {
        if start >= self.nbits { return None; }
        let start_word = start / 64;
        let start_bit = start % 64;
        let first_mask = if start_bit == 0 { u64::MAX } else { u64::MAX << start_bit };
        let first_val = (!self.words[start_word]) & first_mask;
        if first_val != 0 {
            let pos = start_word * 64 + first_val.trailing_zeros() as usize;
            return if pos < self.nbits { Some(pos) } else { None };
        }
        for w in (start_word + 1)..words_for(self.nbits) {
            let v = !self.words[w];
            if v != 0 {
                let pos = w * 64 + v.trailing_zeros() as usize;
                return if pos < self.nbits { Some(pos) } else { None };
            }
        }
        None
    }

    pub fn find_next_set(&self, start: usize) -> Option<usize> {
        if start >= self.nbits { return None; }
        let start_word = start / 64;
        let start_bit = start % 64;
        let first_mask = if start_bit == 0 { u64::MAX } else { u64::MAX << start_bit };
        let first_val = self.words[start_word] & first_mask;
        if first_val != 0 {
            let pos = start_word * 64 + first_val.trailing_zeros() as usize;
            return if pos < self.nbits { Some(pos) } else { None };
        }
        for w in (start_word + 1)..words_for(self.nbits) {
            let v = self.words[w];
            if v != 0 {
                let pos = w * 64 + v.trailing_zeros() as usize;
                return if pos < self.nbits { Some(pos) } else { None };
            }
        }
        None
    }

    pub fn find_first_zero(&self) -> Option<usize> {
        self.find_next_zero(0)
    }

    pub fn find_first_set(&self) -> Option<usize> {
        self.find_next_set(0)
    }

    pub fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn count_zeros(&self) -> usize {
        self.nbits - self.count_ones()
    }

    pub fn clear_all(&mut self) {
        for w in self.words.iter_mut() { *w = 0; }
    }

    pub fn and(&mut self, other: &Self) {
        debug_assert_eq!(self.nbits, other.nbits);
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a &= b;
        }
    }

    pub fn or(&mut self, other: &Self) {
        debug_assert_eq!(self.nbits, other.nbits);
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a |= b;
        }
    }

    pub fn xor(&mut self, other: &Self) {
        debug_assert_eq!(self.nbits, other.nbits);
        for (a, b) in self.words.iter_mut().zip(other.words.iter()) {
            *a ^= b;
        }
    }

    pub fn not(&mut self) {
        for w in self.words.iter_mut() { *w = !*w; }
        if self.nbits % 64 != 0 {
            let last = words_for(self.nbits) - 1;
            self.words[last] &= (1u64 << (self.nbits % 64)) - 1;
        }
    }

    pub fn iter_set(&self) -> DynBitIterator<'_> {
        DynBitIterator { bm: self, next: 0 }
    }
}

/// Iterator over set bit indices in a `DynBitmap`.
pub struct DynBitIterator<'a> {
    bm: &'a DynBitmap,
    next: usize,
}

impl<'a> Iterator for DynBitIterator<'a> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        let bit = self.bm.find_next_set(self.next)?;
        self.next = bit + 1;
        Some(bit)
    }
}

impl fmt::Debug for DynBitmap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DynBitmap({})[", self.nbits)?;
        for bit in 0..self.nbits {
            if self.test(bit) {
                write!(f, "{},", bit)?;
            }
        }
        write!(f, "]")
    }
}
