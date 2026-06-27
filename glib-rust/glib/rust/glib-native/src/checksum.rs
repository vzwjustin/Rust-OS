//! Checksum (digest) computation (`gchecksum`).
//!
//! Supports MD5, SHA-1, SHA-256, SHA-384, and SHA-512.

use crate::bytes::Bytes;
use crate::prelude::*;

/// Supported checksum algorithms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChecksumType {
    /// MD5 algorithm.
    Md5,
    /// SHA-1 algorithm.
    Sha1,
    /// SHA-256 algorithm.
    Sha256,
    /// SHA-512 algorithm.
    Sha512,
    /// SHA-384 algorithm.
    Sha384,
}

/// Returns the length in bytes of the digest for `checksum_type`, or `None`
/// if the type is invalid.
pub fn checksum_type_get_length(checksum_type: ChecksumType) -> Option<usize> {
    match checksum_type {
        ChecksumType::Md5 => Some(16),
        ChecksumType::Sha1 => Some(20),
        ChecksumType::Sha256 => Some(32),
        ChecksumType::Sha512 => Some(64),
        ChecksumType::Sha384 => Some(48),
    }
}

// ---------------------------------------------------------------------------
// MD5
// ---------------------------------------------------------------------------

const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

const MD5_S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

#[derive(Clone)]
struct Md5 {
    state: [u32; 4],
    bit_count: u64,
    buffer: [u8; 64],
    buf_len: usize,
}

impl Md5 {
    fn new() -> Self {
        Self {
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            bit_count: 0,
            buffer: [0; 64],
            buf_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.bit_count = self
            .bit_count
            .wrapping_add((data.len() as u64).wrapping_mul(8));

        let mut data = data;
        if self.buf_len > 0 {
            let need = 64 - self.buf_len;
            if data.len() < need {
                self.buffer[self.buf_len..self.buf_len + data.len()].copy_from_slice(data);
                self.buf_len += data.len();
                return;
            }
            self.buffer[self.buf_len..].copy_from_slice(&data[..need]);
            let block = self.buffer;
            self.process_block(&block);
            data = &data[need..];
            self.buf_len = 0;
        }
        while data.len() >= 64 {
            let block: [u8; 64] = data[..64].try_into().unwrap();
            self.process_block(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        for i in 0..64 {
            let (f, g) = if i < 16 {
                ((b & c) | (!b & d), i)
            } else if i < 32 {
                ((d & b) | (!d & c), (5 * i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3 * i + 5) % 16)
            } else {
                (c ^ (b | !d), (7 * i) % 16)
            };
            let temp = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(MD5_K[i])
                    .wrapping_add(m[g])
                    .rotate_left(MD5_S[i]),
            );
            a = temp;
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }

    fn finalize(mut self) -> Vec<u8> {
        let bit_count = self.bit_count;
        let mut padding = [0u8; 64];
        padding[0] = 0x80;
        let pad_len = if self.buf_len < 56 {
            56 - self.buf_len
        } else {
            120 - self.buf_len
        };
        self.update(&padding[..pad_len]);
        let len_bytes = bit_count.to_le_bytes();
        self.update(&len_bytes);
        let mut result = Vec::with_capacity(16);
        for &v in &self.state {
            result.extend_from_slice(&v.to_le_bytes());
        }
        result
    }
}

// ---------------------------------------------------------------------------
// SHA-1
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Sha1 {
    state: [u32; 5],
    bit_count: u64,
    buffer: [u8; 64],
    buf_len: usize,
}

impl Sha1 {
    fn new() -> Self {
        Self {
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0],
            bit_count: 0,
            buffer: [0; 64],
            buf_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.bit_count = self
            .bit_count
            .wrapping_add((data.len() as u64).wrapping_mul(8));
        let mut data = data;
        if self.buf_len > 0 {
            let need = 64 - self.buf_len;
            if data.len() < need {
                self.buffer[self.buf_len..self.buf_len + data.len()].copy_from_slice(data);
                self.buf_len += data.len();
                return;
            }
            self.buffer[self.buf_len..].copy_from_slice(&data[..need]);
            let block = self.buffer;
            self.process_block(&block);
            data = &data[need..];
            self.buf_len = 0;
        }
        while data.len() >= 64 {
            let block: [u8; 64] = data[..64].try_into().unwrap();
            self.process_block(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = if i < 20 {
                ((b & c) | (!b & d), 0x5a827999u32)
            } else if i < 40 {
                (b ^ c ^ d, 0x6ed9eba1u32)
            } else if i < 60 {
                ((b & c) | (b & d) | (c & d), 0x8f1bbcdcu32)
            } else {
                (b ^ c ^ d, 0xca62c1d6u32)
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
    }

    fn finalize(mut self) -> Vec<u8> {
        let bit_count = self.bit_count;
        let mut padding = [0u8; 64];
        padding[0] = 0x80;
        let pad_len = if self.buf_len < 56 {
            56 - self.buf_len
        } else {
            120 - self.buf_len
        };
        self.update(&padding[..pad_len]);
        let len_bytes = bit_count.to_be_bytes();
        self.update(&len_bytes);
        let mut result = Vec::with_capacity(20);
        for &v in &self.state {
            result.extend_from_slice(&v.to_be_bytes());
        }
        result
    }
}

// ---------------------------------------------------------------------------
// SHA-256
// ---------------------------------------------------------------------------

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[derive(Clone)]
struct Sha256 {
    state: [u32; 8],
    bit_count: u64,
    buffer: [u8; 64],
    buf_len: usize,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            bit_count: 0,
            buffer: [0; 64],
            buf_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.bit_count = self
            .bit_count
            .wrapping_add((data.len() as u64).wrapping_mul(8));
        let mut data = data;
        if self.buf_len > 0 {
            let need = 64 - self.buf_len;
            if data.len() < need {
                self.buffer[self.buf_len..self.buf_len + data.len()].copy_from_slice(data);
                self.buf_len += data.len();
                return;
            }
            self.buffer[self.buf_len..].copy_from_slice(&data[..need]);
            let block = self.buffer;
            self.process_block(&block);
            data = &data[need..];
            self.buf_len = 0;
        }
        while data.len() >= 64 {
            let block: [u8; 64] = data[..64].try_into().unwrap();
            self.process_block(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    fn finalize(mut self) -> Vec<u8> {
        let bit_count = self.bit_count;
        let mut padding = [0u8; 64];
        padding[0] = 0x80;
        let pad_len = if self.buf_len < 56 {
            56 - self.buf_len
        } else {
            120 - self.buf_len
        };
        self.update(&padding[..pad_len]);
        let len_bytes = bit_count.to_be_bytes();
        self.update(&len_bytes);
        let mut result = Vec::with_capacity(32);
        for &v in &self.state {
            result.extend_from_slice(&v.to_be_bytes());
        }
        result
    }
}

// ---------------------------------------------------------------------------
// SHA-512 / SHA-384
// ---------------------------------------------------------------------------

const SHA512_K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

#[derive(Clone)]
struct Sha512 {
    state: [u64; 8],
    bit_count: u128,
    buffer: [u8; 128],
    buf_len: usize,
    is_384: bool,
}

impl Sha512 {
    fn new_sha512() -> Self {
        Self {
            state: [
                0x6a09e667f3bcc908,
                0xbb67ae8584caa73b,
                0x3c6ef372fe94f82b,
                0xa54ff53a5f1d36f1,
                0x510e527fade682d1,
                0x9b05688c2b3e6c1f,
                0x1f83d9abfb41bd6b,
                0x5be0cd19137e2179,
            ],
            bit_count: 0,
            buffer: [0; 128],
            buf_len: 0,
            is_384: false,
        }
    }

    fn new_sha384() -> Self {
        Self {
            state: [
                0xcbbb9d5dc1059ed8,
                0x629a292a367cd507,
                0x9159015a3070dd17,
                0x152fecd8f70e5939,
                0x67332667ffc00b31,
                0x8eb44a8768581511,
                0xdb0c2e0d64f98fa7,
                0x47b5481dbefa4fa4,
            ],
            bit_count: 0,
            buffer: [0; 128],
            buf_len: 0,
            is_384: true,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.bit_count = self
            .bit_count
            .wrapping_add((data.len() as u128).wrapping_mul(8));
        let mut data = data;
        if self.buf_len > 0 {
            let need = 128 - self.buf_len;
            if data.len() < need {
                self.buffer[self.buf_len..self.buf_len + data.len()].copy_from_slice(data);
                self.buf_len += data.len();
                return;
            }
            self.buffer[self.buf_len..].copy_from_slice(&data[..need]);
            let block = self.buffer;
            self.process_block(&block);
            data = &data[need..];
            self.buf_len = 0;
        }
        while data.len() >= 128 {
            let block: [u8; 128] = data[..128].try_into().unwrap();
            self.process_block(&block);
            data = &data[128..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn process_block(&mut self, block: &[u8; 128]) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes([
                block[i * 8],
                block[i * 8 + 1],
                block[i * 8 + 2],
                block[i * 8 + 3],
                block[i * 8 + 4],
                block[i * 8 + 5],
                block[i * 8 + 6],
                block[i * 8 + 7],
            ]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];
        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA512_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    fn finalize(mut self) -> Vec<u8> {
        let bit_count = self.bit_count;
        let mut padding = [0u8; 128];
        padding[0] = 0x80;
        let pad_len = if self.buf_len < 112 {
            112 - self.buf_len
        } else {
            240 - self.buf_len
        };
        self.update(&padding[..pad_len]);
        let len_bytes = bit_count.to_be_bytes();
        self.update(&len_bytes);
        let full_len = if self.is_384 { 48 } else { 64 };
        let mut result = Vec::with_capacity(full_len);
        for i in 0..8 {
            if result.len() >= full_len {
                break;
            }
            let bytes = self.state[i].to_be_bytes();
            let take = 8.min(full_len - result.len());
            result.extend_from_slice(&bytes[..take]);
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Checksum
// ---------------------------------------------------------------------------

/// Opaque checksum state for incremental digest computation.
#[derive(Clone)]
pub struct Checksum {
    checksum_type: ChecksumType,
    engine: ChecksumEngine,
}

#[derive(Clone)]
enum ChecksumEngine {
    Md5(Md5),
    Sha1(Sha1),
    Sha256(Sha256),
    Sha512(Sha512),
}

impl Checksum {
    /// Create a new `Checksum` for the given algorithm.
    pub fn new(checksum_type: ChecksumType) -> Self {
        let engine = match checksum_type {
            ChecksumType::Md5 => ChecksumEngine::Md5(Md5::new()),
            ChecksumType::Sha1 => ChecksumEngine::Sha1(Sha1::new()),
            ChecksumType::Sha256 => ChecksumEngine::Sha256(Sha256::new()),
            ChecksumType::Sha512 => ChecksumEngine::Sha512(Sha512::new_sha512()),
            ChecksumType::Sha384 => ChecksumEngine::Sha512(Sha512::new_sha384()),
        };
        Self {
            checksum_type,
            engine,
        }
    }

    /// Reset the checksum to its initial state.
    pub fn reset(&mut self) {
        self.engine = match self.checksum_type {
            ChecksumType::Md5 => ChecksumEngine::Md5(Md5::new()),
            ChecksumType::Sha1 => ChecksumEngine::Sha1(Sha1::new()),
            ChecksumType::Sha256 => ChecksumEngine::Sha256(Sha256::new()),
            ChecksumType::Sha512 => ChecksumEngine::Sha512(Sha512::new_sha512()),
            ChecksumType::Sha384 => ChecksumEngine::Sha512(Sha512::new_sha384()),
        };
    }

    /// Feed data into the checksum.
    pub fn update(&mut self, data: &[u8]) {
        match &mut self.engine {
            ChecksumEngine::Md5(e) => e.update(data),
            ChecksumEngine::Sha1(e) => e.update(data),
            ChecksumEngine::Sha256(e) => e.update(data),
            ChecksumEngine::Sha512(e) => e.update(data),
        }
    }

    /// Get the digest as a lowercase hexadecimal string.
    pub fn get_string(&self) -> String {
        let digest = self.clone().into_digest();
        hex_encode(&digest)
    }

    /// Get the raw digest bytes into `buffer`. Returns the number of bytes
    /// written. If `buffer` is smaller than the digest, only `buffer.len()`
    /// bytes are written.
    pub fn get_digest(&self, buffer: &mut [u8]) -> usize {
        let digest = self.clone().into_digest();
        let len = digest.len().min(buffer.len());
        buffer[..len].copy_from_slice(&digest[..len]);
        len
    }

    fn into_digest(self) -> Vec<u8> {
        match self.engine {
            ChecksumEngine::Md5(e) => e.finalize(),
            ChecksumEngine::Sha1(e) => e.finalize(),
            ChecksumEngine::Sha256(e) => e.finalize(),
            ChecksumEngine::Sha512(e) => e.finalize(),
        }
    }
}

fn hex_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for &b in data {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Compute the checksum of `data` as a hex string (`g_compute_checksum_for_data`).
pub fn compute_checksum_for_data(checksum_type: ChecksumType, data: &[u8]) -> String {
    let mut cs = Checksum::new(checksum_type);
    cs.update(data);
    cs.get_string()
}

/// Compute the checksum of a string as a hex string (`g_compute_checksum_for_string`).
pub fn compute_checksum_for_string(checksum_type: ChecksumType, s: &str) -> String {
    compute_checksum_for_data(checksum_type, s.as_bytes())
}

/// Compute the checksum of a `Bytes` as a hex string (`g_compute_checksum_for_bytes`).
pub fn compute_checksum_for_bytes(checksum_type: ChecksumType, data: &Bytes) -> String {
    compute_checksum_for_data(checksum_type, data.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED_STR: &str = "The quick brown fox jumps over the lazy dog. Jackdaws love my big sphinx of quartz. Pack my box with five dozen liquor jugs. How razorback-jumping frogs can level six piqued gymnasts!";

    #[test]
    fn type_get_length() {
        assert_eq!(checksum_type_get_length(ChecksumType::Md5), Some(16));
        assert_eq!(checksum_type_get_length(ChecksumType::Sha1), Some(20));
        assert_eq!(checksum_type_get_length(ChecksumType::Sha256), Some(32));
        assert_eq!(checksum_type_get_length(ChecksumType::Sha384), Some(48));
        assert_eq!(checksum_type_get_length(ChecksumType::Sha512), Some(64));
    }

    #[test]
    fn md5_empty() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Md5, b""),
            "d41d8cd98f00b204e9800998ecf8427e"
        );
    }

    #[test]
    fn md5_abc() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Md5, b"abc"),
            "900150983cd24fb0d6963f7d28e17f72"
        );
    }

    #[test]
    fn md5_quick_fox() {
        assert_eq!(
            compute_checksum_for_string(
                ChecksumType::Md5,
                "The quick brown fox jumps over the lazy dog"
            ),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
    }

    #[test]
    fn md5_fixed_str() {
        // Last entry from GLib's MD5_sums array
        assert_eq!(
            compute_checksum_for_string(ChecksumType::Md5, FIXED_STR),
            "407b72260377f77f8e63e13dc09bda2c"
        );
    }

    #[test]
    fn sha1_empty() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Sha1, b""),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn sha1_abc() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Sha1, b"abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn sha1_quick_fox() {
        assert_eq!(
            compute_checksum_for_string(
                ChecksumType::Sha1,
                "The quick brown fox jumps over the lazy dog"
            ),
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"
        );
    }

    #[test]
    fn sha1_fixed_str() {
        assert_eq!(
            compute_checksum_for_string(ChecksumType::Sha1, FIXED_STR),
            "8802f1d217906250585b75187b1ebfbb5c6cbcae"
        );
    }

    #[test]
    fn sha256_empty() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Sha256, b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_abc() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Sha256, b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_quick_fox() {
        assert_eq!(
            compute_checksum_for_string(
                ChecksumType::Sha256,
                "The quick brown fox jumps over the lazy dog"
            ),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }

    #[test]
    fn sha256_fixed_str() {
        assert_eq!(
            compute_checksum_for_string(ChecksumType::Sha256, FIXED_STR),
            "df3a0c35d5345d6d792415c1310bd4589cdf68bac96ed599d6bb0c1545ffc86c"
        );
    }

    #[test]
    fn sha384_empty() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Sha384, b""),
            "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b"
        );
    }

    #[test]
    fn sha384_abc() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Sha384, b"abc"),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
    }

    #[test]
    fn sha384_fixed_str() {
        assert_eq!(
            compute_checksum_for_string(ChecksumType::Sha384, FIXED_STR),
            "396d84c9c1a2ee76b0163c38533cbc8bc453089e87b9790a62bf5175e614713fea4f16378b416fd8650351345cd44c07"
        );
    }

    #[test]
    fn sha512_empty() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Sha512, b""),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
    }

    #[test]
    fn sha512_abc() {
        assert_eq!(
            compute_checksum_for_data(ChecksumType::Sha512, b"abc"),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn sha512_fixed_str() {
        assert_eq!(
            compute_checksum_for_string(ChecksumType::Sha512, FIXED_STR),
            "9da644c289075656b5339317f7100d954b49e67e6c3f981451bf7982c52f003016470c781fa0af61a965fc0ae50f1bbc8d94ffe91e10dc09f27dbe5b1fc2827c"
        );
    }

    #[test]
    fn incremental_update() {
        let data = FIXED_STR.as_bytes();
        for chunk_size in [1, 2, 3, 4, 5, 7, 13, 32, 64, 65, 100, 128, 200] {
            let mut cs = Checksum::new(ChecksumType::Sha256);
            let mut offset = 0;
            while offset < data.len() {
                let end = (offset + chunk_size).min(data.len());
                cs.update(&data[offset..end]);
                offset = end;
            }
            assert_eq!(
                cs.get_string(),
                compute_checksum_for_data(ChecksumType::Sha256, data),
                "chunk_size={chunk_size}"
            );
        }
    }

    #[test]
    fn incremental_all_types() {
        let data = FIXED_STR.as_bytes();
        let types = [
            ChecksumType::Md5,
            ChecksumType::Sha1,
            ChecksumType::Sha256,
            ChecksumType::Sha384,
            ChecksumType::Sha512,
        ];
        for ct in types {
            for chunk_size in [1, 3, 7, 64, 128] {
                let mut cs = Checksum::new(ct);
                let mut offset = 0;
                while offset < data.len() {
                    let end = (offset + chunk_size).min(data.len());
                    cs.update(&data[offset..end]);
                    offset = end;
                }
                assert_eq!(
                    cs.get_string(),
                    compute_checksum_for_data(ct, data),
                    "type={ct:?} chunk_size={chunk_size}"
                );
            }
        }
    }

    #[test]
    fn reset() {
        let mut cs = Checksum::new(ChecksumType::Md5);
        cs.update(b"hello");
        cs.reset();
        cs.update(b"");
        assert_eq!(cs.get_string(), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn copy() {
        let mut cs1 = Checksum::new(ChecksumType::Sha256);
        cs1.update(b"hello ");
        let mut cs2 = cs1.clone();
        cs1.update(b"world");
        cs2.update(b"world");
        assert_eq!(cs1.get_string(), cs2.get_string());
    }

    #[test]
    fn get_digest() {
        let mut cs = Checksum::new(ChecksumType::Md5);
        cs.update(b"abc");
        let mut buf = [0u8; 16];
        let len = cs.get_digest(&mut buf);
        assert_eq!(len, 16);
        assert_eq!(hex_encode(&buf), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn get_digest_small_buffer() {
        let mut cs = Checksum::new(ChecksumType::Sha256);
        cs.update(b"abc");
        let mut buf = [0u8; 10];
        let len = cs.get_digest(&mut buf);
        assert_eq!(len, 10);
        assert_eq!(hex_encode(&buf), "ba7816bf8f01cfea4141");
    }
}
