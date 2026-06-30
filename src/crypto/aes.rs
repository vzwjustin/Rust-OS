//! AES-128/256 CBC block cipher.

use super::algapi::CryptoError;
use alloc::vec::Vec;

pub const BLOCK_SIZE: usize = 16;

const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

const INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

const RCON: [u32; 11] = [
    0x00000000, 0x01000000, 0x02000000, 0x04000000, 0x08000000, 0x10000000, 0x20000000, 0x40000000,
    0x80000000, 0x1b000000, 0x36000000,
];

pub(crate) struct AesContext {
    round_keys: Vec<u32>,
    rounds: usize,
}

fn validate_cbc_inputs(key: &[u8], iv: &[u8], data: &[u8]) -> Result<(), CryptoError> {
    if iv.len() != BLOCK_SIZE {
        return Err(CryptoError::InvalidIvLength);
    }
    if data.len() % BLOCK_SIZE != 0 {
        return Err(CryptoError::InvalidBlockAlignment);
    }
    if key.len() != 16 && key.len() != 32 {
        return Err(CryptoError::InvalidKeySize);
    }
    Ok(())
}

fn sub_word(word: u32) -> u32 {
    let b0 = SBOX[((word >> 24) & 0xFF) as usize] as u32;
    let b1 = SBOX[((word >> 16) & 0xFF) as usize] as u32;
    let b2 = SBOX[((word >> 8) & 0xFF) as usize] as u32;
    let b3 = SBOX[(word & 0xFF) as usize] as u32;
    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
}

fn rot_word(word: u32) -> u32 {
    (word << 8) | (word >> 24)
}

pub(crate) fn expand_key(key: &[u8]) -> AesContext {
    let nk = key.len() / 4;
    let rounds = nk + 6;
    let key_words = 4 * (rounds + 1);
    let mut w = Vec::with_capacity(key_words);

    for i in 0..nk {
        w.push(u32::from_be_bytes([
            key[i * 4],
            key[i * 4 + 1],
            key[i * 4 + 2],
            key[i * 4 + 3],
        ]));
    }

    for i in nk..key_words {
        let mut temp = w[i - 1];
        if i % nk == 0 {
            temp = sub_word(rot_word(temp)) ^ RCON[i / nk];
        } else if nk > 6 && i % nk == 4 {
            temp = sub_word(temp);
        }
        w.push(w[i - nk] ^ temp);
    }

    AesContext {
        round_keys: w,
        rounds,
    }
}

fn add_round_key(state: &mut [u8; BLOCK_SIZE], round_key: &[u32]) {
    for (i, rk) in round_key.iter().enumerate() {
        let bytes = rk.to_be_bytes();
        state[i * 4] ^= bytes[0];
        state[i * 4 + 1] ^= bytes[1];
        state[i * 4 + 2] ^= bytes[2];
        state[i * 4 + 3] ^= bytes[3];
    }
}

fn sub_bytes(state: &mut [u8; BLOCK_SIZE]) {
    for byte in state.iter_mut() {
        *byte = SBOX[*byte as usize];
    }
}

fn inv_sub_bytes(state: &mut [u8; BLOCK_SIZE]) {
    for byte in state.iter_mut() {
        *byte = INV_SBOX[*byte as usize];
    }
}

fn shift_rows(state: &mut [u8; BLOCK_SIZE]) {
    let t = *state;
    state[1] = t[5];
    state[5] = t[9];
    state[9] = t[13];
    state[13] = t[1];
    state[2] = t[10];
    state[6] = t[14];
    state[10] = t[2];
    state[14] = t[6];
    state[3] = t[15];
    state[7] = t[3];
    state[11] = t[7];
    state[15] = t[11];
}

fn inv_shift_rows(state: &mut [u8; BLOCK_SIZE]) {
    let t = *state;
    state[1] = t[13];
    state[5] = t[1];
    state[9] = t[5];
    state[13] = t[9];
    state[2] = t[10];
    state[6] = t[14];
    state[10] = t[2];
    state[14] = t[6];
    state[3] = t[7];
    state[7] = t[11];
    state[11] = t[15];
    state[15] = t[3];
}

fn galois_mul(a: u8, b: u8) -> u8 {
    let mut p = 0u8;
    let mut aa = a;
    let mut bb = b;
    for _ in 0..8 {
        if (bb & 1) != 0 {
            p ^= aa;
        }
        let hi = (aa & 0x80) != 0;
        aa <<= 1;
        if hi {
            aa ^= 0x1B;
        }
        bb >>= 1;
    }
    p
}

fn mix_columns(state: &mut [u8; BLOCK_SIZE]) {
    let t = *state;
    for c in 0..4 {
        let o = c * 4;
        let s0 = t[o];
        let s1 = t[o + 1];
        let s2 = t[o + 2];
        let s3 = t[o + 3];
        state[o] = galois_mul(0x02, s0) ^ galois_mul(0x03, s1) ^ s2 ^ s3;
        state[o + 1] = s0 ^ galois_mul(0x02, s1) ^ galois_mul(0x03, s2) ^ s3;
        state[o + 2] = s0 ^ s1 ^ galois_mul(0x02, s2) ^ galois_mul(0x03, s3);
        state[o + 3] = galois_mul(0x03, s0) ^ s1 ^ s2 ^ galois_mul(0x02, s3);
    }
}

fn inv_mix_columns(state: &mut [u8; BLOCK_SIZE]) {
    let t = *state;
    for c in 0..4 {
        let o = c * 4;
        let s0 = t[o];
        let s1 = t[o + 1];
        let s2 = t[o + 2];
        let s3 = t[o + 3];
        state[o] = galois_mul(0x0e, s0)
            ^ galois_mul(0x0b, s1)
            ^ galois_mul(0x0d, s2)
            ^ galois_mul(0x09, s3);
        state[o + 1] = galois_mul(0x09, s0)
            ^ galois_mul(0x0e, s1)
            ^ galois_mul(0x0b, s2)
            ^ galois_mul(0x0d, s3);
        state[o + 2] = galois_mul(0x0d, s0)
            ^ galois_mul(0x09, s1)
            ^ galois_mul(0x0e, s2)
            ^ galois_mul(0x0b, s3);
        state[o + 3] = galois_mul(0x0b, s0)
            ^ galois_mul(0x0d, s1)
            ^ galois_mul(0x09, s2)
            ^ galois_mul(0x0e, s3);
    }
}

pub(crate) fn encrypt_block(ctx: &AesContext, block: &[u8; BLOCK_SIZE]) -> [u8; BLOCK_SIZE] {
    let mut state = *block;
    add_round_key(&mut state, &ctx.round_keys[0..4]);

    for round in 1..ctx.rounds {
        sub_bytes(&mut state);
        shift_rows(&mut state);
        mix_columns(&mut state);
        let start = round * 4;
        add_round_key(&mut state, &ctx.round_keys[start..start + 4]);
    }

    sub_bytes(&mut state);
    shift_rows(&mut state);
    let last = ctx.rounds * 4;
    add_round_key(&mut state, &ctx.round_keys[last..last + 4]);
    state
}

fn decrypt_block(ctx: &AesContext, block: &[u8; BLOCK_SIZE]) -> [u8; BLOCK_SIZE] {
    let mut state = *block;
    let last = ctx.rounds * 4;
    add_round_key(&mut state, &ctx.round_keys[last..last + 4]);

    for round in (1..ctx.rounds).rev() {
        inv_shift_rows(&mut state);
        inv_sub_bytes(&mut state);
        let start = round * 4;
        add_round_key(&mut state, &ctx.round_keys[start..start + 4]);
        inv_mix_columns(&mut state);
    }

    inv_shift_rows(&mut state);
    inv_sub_bytes(&mut state);
    add_round_key(&mut state, &ctx.round_keys[0..4]);
    state
}

fn xor_block(a: &mut [u8; BLOCK_SIZE], b: &[u8; BLOCK_SIZE]) {
    for i in 0..BLOCK_SIZE {
        a[i] ^= b[i];
    }
}

/// AES-CBC encrypt. `key` is 16 or 32 bytes; `iv` and data are block-aligned.
pub fn cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    validate_cbc_inputs(key, iv, plaintext)?;
    let ctx = expand_key(key);
    let mut out = Vec::with_capacity(plaintext.len());
    let mut chain = {
        let mut iv_block = [0u8; BLOCK_SIZE];
        iv_block.copy_from_slice(iv);
        iv_block
    };

    for chunk in plaintext.chunks(BLOCK_SIZE) {
        let mut block = [0u8; BLOCK_SIZE];
        block.copy_from_slice(chunk);
        xor_block(&mut block, &chain);
        let enc = encrypt_block(&ctx, &block);
        out.extend_from_slice(&enc);
        chain = enc;
    }
    Ok(out)
}

/// AES-CBC decrypt. `key` is 16 or 32 bytes; `iv` and data are block-aligned.
pub fn cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    validate_cbc_inputs(key, iv, ciphertext)?;
    let ctx = expand_key(key);
    let mut out = Vec::with_capacity(ciphertext.len());
    let mut chain = {
        let mut iv_block = [0u8; BLOCK_SIZE];
        iv_block.copy_from_slice(iv);
        iv_block
    };

    for chunk in ciphertext.chunks(BLOCK_SIZE) {
        let mut block = [0u8; BLOCK_SIZE];
        block.copy_from_slice(chunk);
        let plain = decrypt_block(&ctx, &block);
        let mut result = plain;
        xor_block(&mut result, &chain);
        out.extend_from_slice(&result);
        chain = block;
    }
    Ok(out)
}

/// Registry wrapper: AES-128-CBC only accepts 16-byte keys.
pub fn aes128_cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if key.len() != 16 {
        return Err(CryptoError::InvalidKeySize);
    }
    cbc_encrypt(key, iv, plaintext)
}

pub fn aes128_cbc_decrypt(
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if key.len() != 16 {
        return Err(CryptoError::InvalidKeySize);
    }
    cbc_decrypt(key, iv, ciphertext)
}

/// Registry wrapper: AES-256-CBC only accepts 32-byte keys.
pub fn aes256_cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if key.len() != 32 {
        return Err(CryptoError::InvalidKeySize);
    }
    cbc_encrypt(key, iv, plaintext)
}

pub fn aes256_cbc_decrypt(
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if key.len() != 32 {
        return Err(CryptoError::InvalidKeySize);
    }
    cbc_decrypt(key, iv, ciphertext)
}
