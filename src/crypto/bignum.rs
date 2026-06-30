//! Minimal arbitrary-precision unsigned integer arithmetic, just enough to
//! perform RSA public-key modular exponentiation (`m = c^e mod n`) for
//! PKCS#1 v1.5 signature verification. Not constant-time and not optimized
//! (schoolbook multiply + bit-serial long division) — acceptable here
//! because RSA *verification* uses a small public exponent (commonly
//! 65537) and only runs at module-load / cert-import time, never in a hot
//! path, and verification of public data has no timing-attack surface.

use alloc::vec;
use alloc::vec::Vec;

/// Little-endian base-2^32 limb representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BigUint {
    limbs: Vec<u32>,
}

impl BigUint {
    pub fn zero() -> Self {
        BigUint { limbs: vec![0] }
    }

    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut limbs = Vec::with_capacity(bytes.len() / 4 + 1);
        let mut chunks = bytes.rchunks(4);
        while let Some(chunk) = chunks.next() {
            let mut buf = [0u8; 4];
            buf[4 - chunk.len()..].copy_from_slice(chunk);
            limbs.push(u32::from_be_bytes(buf));
        }
        if limbs.is_empty() {
            limbs.push(0);
        }
        let mut v = BigUint { limbs };
        v.trim();
        v
    }

    /// Render as big-endian bytes, left-padded with zeros to exactly `len`
    /// bytes. Returns `None` if the value doesn't fit.
    pub fn to_be_bytes_padded(&self, len: usize) -> Option<Vec<u8>> {
        let mut out = vec![0u8; self.limbs.len() * 4];
        for (i, limb) in self.limbs.iter().enumerate() {
            let b = limb.to_le_bytes();
            out[i * 4..i * 4 + 4].copy_from_slice(&b);
        }
        out.reverse(); // now big-endian, may have leading zero limbs' bytes
        // strip leading zero bytes
        let first_nonzero = out.iter().position(|&b| b != 0).unwrap_or(out.len() - 1);
        let trimmed = &out[first_nonzero..];
        if trimmed.len() > len {
            return None;
        }
        let mut result = vec![0u8; len];
        result[len - trimmed.len()..].copy_from_slice(trimmed);
        Some(result)
    }

    fn trim(&mut self) {
        while self.limbs.len() > 1 && *self.limbs.last().unwrap() == 0 {
            self.limbs.pop();
        }
    }

    pub fn is_zero(&self) -> bool {
        self.limbs.iter().all(|&l| l == 0)
    }

    fn bit_len(&self) -> usize {
        let top = *self.limbs.last().unwrap();
        if top == 0 {
            return 0;
        }
        self.limbs.len() * 32 - top.leading_zeros() as usize
    }

    fn get_bit(&self, i: usize) -> bool {
        let limb = i / 32;
        let bit = i % 32;
        if limb >= self.limbs.len() {
            false
        } else {
            (self.limbs[limb] >> bit) & 1 != 0
        }
    }

    fn cmp(&self, other: &BigUint) -> core::cmp::Ordering {
        let a_len = self.limbs.len();
        let b_len = other.limbs.len();
        if a_len != b_len {
            // both trimmed, so longer means greater
            return a_len.cmp(&b_len);
        }
        for i in (0..a_len).rev() {
            if self.limbs[i] != other.limbs[i] {
                return self.limbs[i].cmp(&other.limbs[i]);
            }
        }
        core::cmp::Ordering::Equal
    }

    fn sub_assign(&mut self, other: &BigUint) {
        let mut borrow: i64 = 0;
        for i in 0..self.limbs.len() {
            let b = if i < other.limbs.len() {
                other.limbs[i] as i64
            } else {
                0
            };
            let mut v = self.limbs[i] as i64 - b - borrow;
            if v < 0 {
                v += 1i64 << 32;
                borrow = 1;
            } else {
                borrow = 0;
            }
            self.limbs[i] = v as u32;
        }
        self.trim();
    }

    fn shl1(&mut self) {
        let mut carry = 0u32;
        for limb in self.limbs.iter_mut() {
            let new_carry = *limb >> 31;
            *limb = (*limb << 1) | carry;
            carry = new_carry;
        }
        if carry != 0 {
            self.limbs.push(carry);
        }
    }

    fn or_bit0(&mut self, bit: bool) {
        if bit {
            self.limbs[0] |= 1;
        }
    }

    fn mul(&self, other: &BigUint) -> BigUint {
        let mut result = vec![0u64; self.limbs.len() + other.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            if a == 0 {
                continue;
            }
            let mut carry = 0u64;
            for (j, &b) in other.limbs.iter().enumerate() {
                let idx = i + j;
                let prod = a as u64 * b as u64 + result[idx] + carry;
                result[idx] = prod & 0xFFFF_FFFF;
                carry = prod >> 32;
            }
            let mut k = i + other.limbs.len();
            while carry != 0 {
                let sum = result[k] + carry;
                result[k] = sum & 0xFFFF_FFFF;
                carry = sum >> 32;
                k += 1;
            }
        }
        let limbs: Vec<u32> = result.iter().map(|&v| v as u32).collect();
        let mut v = BigUint { limbs };
        v.trim();
        v
    }

    /// Bit-serial long division remainder: self mod modulus.
    fn rem(&self, modulus: &BigUint) -> BigUint {
        let mut remainder = BigUint::zero();
        let bits = self.bit_len();
        for i in (0..bits).rev() {
            remainder.shl1();
            remainder.or_bit0(self.get_bit(i));
            if remainder.cmp(modulus) != core::cmp::Ordering::Less {
                remainder.sub_assign(modulus);
            }
        }
        remainder
    }

    /// Modular exponentiation: self^exp mod modulus, via square-and-multiply.
    pub fn modpow(&self, exp: &BigUint, modulus: &BigUint) -> BigUint {
        let mut result = BigUint::from_be_bytes(&[1]);
        let mut base = self.rem(modulus);
        let bits = exp.bit_len();
        for i in 0..bits {
            if exp.get_bit(i) {
                result = result.mul(&base).rem(modulus);
            }
            base = base.mul(&base).rem(modulus);
        }
        result
    }
}
