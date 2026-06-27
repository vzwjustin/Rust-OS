//! Base64 encoding and decoding (`gbase64`).

use crate::prelude::*;

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const MIME_BASE64_RANK: [u8; 256] = {
    let mut t = [255u8; 256];
    t[b'+' as usize] = 62;
    t[b'/' as usize] = 63;
    // Padding: GLib ranks '=' as 0 so it's counted into the 4-char quad and the
    // `last[]` markers can detect trailing padding. Without this, decode skips
    // '=' and never completes a group → empty output.
    t[b'=' as usize] = 0;
    let mut i = 0;
    while i < 10 {
        t[(b'0' + i) as usize] = 52 + i;
        i += 1;
    }
    let mut i = 0;
    while i < 26 {
        t[(b'A' + i) as usize] = i;
        t[(b'a' + i) as usize] = 26 + i;
        i += 1;
    }
    t
};

/// Encoder state for incremental base64 encoding.
#[derive(Clone, Copy, Debug)]
pub struct Base64Encoder {
    state: i32,
    save: [u8; 3],
}

impl Default for Base64Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Base64Encoder {
    /// Create a new encoder with zeroed state.
    pub const fn new() -> Self {
        Self {
            state: 0,
            save: [0, 0, 0],
        }
    }

    /// Incrementally encode `input` into `output`.
    ///
    /// Returns the number of bytes written to `output`.
    /// `output` must be large enough: at least `(input.len() / 3 + 1) * 4 + 4`
    /// bytes, plus extra for line breaks if enabled.
    pub fn encode_step(
        &mut self,
        input: &[u8],
        break_lines: bool,
        output: &mut [u8],
    ) -> usize {
        if input.is_empty() {
            return 0;
        }

        let mut out_idx = 0;
        let mut in_idx = 0;
        let len = input.len();
        let save_count = self.save[0] as usize;

        if len + save_count > 2 {
            let mut already = self.state;

            // Handle leftover bytes from previous step
            let (mut c1, mut c2);
            let mut c3;

            match save_count {
                1 => {
                    c1 = self.save[1];
                    // fall through to processing
                    c2 = input[in_idx];
                    in_idx += 1;
                    c3 = input[in_idx];
                    in_idx += 1;
                }
                2 => {
                    c1 = self.save[1];
                    c2 = self.save[2];
                    c3 = input[in_idx];
                    in_idx += 1;
                }
                _ => {
                    c1 = input[in_idx];
                    in_idx += 1;
                    c2 = input[in_idx];
                    in_idx += 1;
                    c3 = input[in_idx];
                    in_idx += 1;
                }
            }

            loop {
                output[out_idx] = BASE64_ALPHABET[(c1 >> 2) as usize];
                out_idx += 1;
                output[out_idx] = BASE64_ALPHABET[((c2 >> 4) | ((c1 & 0x3) << 4)) as usize];
                out_idx += 1;
                output[out_idx] =
                    BASE64_ALPHABET[(((c2 & 0x0f) << 2) | (c3 >> 6)) as usize];
                out_idx += 1;
                output[out_idx] = BASE64_ALPHABET[(c3 & 0x3f) as usize];
                out_idx += 1;

                if break_lines {
                    already += 1;
                    if already >= 19 {
                        output[out_idx] = b'\n';
                        out_idx += 1;
                        already = 0;
                    }
                }

                if in_idx + 2 >= len {
                    break;
                }

                c1 = input[in_idx];
                in_idx += 1;
                c2 = input[in_idx];
                in_idx += 1;
                c3 = input[in_idx];
                in_idx += 1;
            }

            // Now in_idx is past the last complete triple processed.
            // Remaining bytes: len - in_idx (0, 1, or 2)
            self.save[0] = 0;
            self.state = already;
        }

        let remaining = len - in_idx;
        debug_assert!(remaining <= 2);

        for i in 0..remaining {
            let slot = self.save[0] as usize + 1 + i;
            self.save[slot] = input[in_idx + i];
        }
        self.save[0] += remaining as u8;

        out_idx
    }

    /// Flush remaining encoder state.
    ///
    /// Returns the number of bytes written to `output` (up to 5 with line break).
    pub fn encode_close(&mut self, break_lines: bool, output: &mut [u8]) -> usize {
        let c1 = self.save[1];
        let c2 = self.save[2];
        let mut out_idx = 0;

        match self.save[0] {
            2 => {
                output[2] = BASE64_ALPHABET[((c2 & 0x0f) << 2) as usize];
                output[0] = BASE64_ALPHABET[(c1 >> 2) as usize];
                output[1] = BASE64_ALPHABET[((c2 >> 4) | ((c1 & 0x3) << 4)) as usize];
                output[3] = b'=';
                out_idx = 4;
            }
            1 => {
                output[0] = BASE64_ALPHABET[(c1 >> 2) as usize];
                output[1] = BASE64_ALPHABET[((c1 & 0x3) << 4) as usize];
                output[2] = b'=';
                output[3] = b'=';
                out_idx = 4;
            }
            _ => {}
        }

        if break_lines {
            output[out_idx] = b'\n';
            out_idx += 1;
        }

        self.save = [0, 0, 0];
        self.state = 0;

        out_idx
    }
}

/// Encode binary data into a Base64 string (`g_base64_encode`).
pub fn base64_encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }

    let allocsize = (data.len() / 3 + 1) * 4 + 1;
    let mut out = vec![0u8; allocsize];
    let mut encoder = Base64Encoder::new();

    let mut outlen = encoder.encode_step(data, false, &mut out);
    outlen += encoder.encode_close(false, &mut out[outlen..]);

    out.truncate(outlen);
    // SAFETY: base64 alphabet is all valid ASCII/UTF-8
    unsafe { String::from_utf8_unchecked(out) }
}

/// Decode a Base64 string into binary data (`g_base64_decode`).
pub fn base64_decode(text: &str) -> Vec<u8> {
    let input = text.as_bytes();
    let mut ret = vec![0u8; (input.len() / 4) * 3 + 1];
    let mut decoder = Base64Decoder::new();
    let decoded_len = decoder.decode_step(input, &mut ret);
    ret.truncate(decoded_len);
    ret
}

/// Decode Base64 in-place, overwriting the input buffer (`g_base64_decode_inplace`).
pub fn base64_decode_inplace(text: &mut [u8]) -> usize {
    if text.len() <= 1 {
        return 0;
    }
    let input_length = text.len();
    let mut decoder = Base64Decoder::new();
    // ponytail: copy the input so we don't borrow `text` immutably and mutably
    // at once; in-place decode never writes past the read cursor, but the borrow
    // checker can't see that. alloc is available.
    let input = text[..input_length].to_vec();
    decoder.decode_step(&input, text)
}

/// Decoder state for incremental base64 decoding.
#[derive(Clone, Copy, Debug)]
pub struct Base64Decoder {
    state: i32,
    save: u32,
}

impl Default for Base64Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Base64Decoder {
    /// Create a new decoder with zeroed state.
    pub const fn new() -> Self {
        Self { state: 0, save: 0 }
    }

    /// Incrementally decode `input` into `output`.
    ///
    /// Returns the number of bytes written to `output`.
    /// `output` must be large enough: at least `(input.len() / 4) * 3 + 3` bytes.
    pub fn decode_step(&mut self, input: &[u8], output: &mut [u8]) -> usize {
        if input.is_empty() {
            return 0;
        }

        let mut v = self.save;
        let mut i = self.state;
        let mut last = [0u8; 2];
        let mut out_idx = 0;

        // Check if we got a padding char in the previous sequence
        if i < 0 {
            i = -i;
            last[0] = b'=';
        }

        for &c in input {
            let rank = MIME_BASE64_RANK[c as usize];
            if rank != 0xff {
                last[1] = last[0];
                last[0] = c;
                v = (v << 6) | rank as u32;
                i += 1;
                if i == 4 {
                    output[out_idx] = (v >> 16) as u8;
                    out_idx += 1;
                    if last[1] != b'=' {
                        output[out_idx] = (v >> 8) as u8;
                        out_idx += 1;
                    }
                    if last[0] != b'=' {
                        output[out_idx] = v as u8;
                        out_idx += 1;
                    }
                    i = 0;
                }
            }
        }

        self.save = v;
        self.state = if last[0] == b'=' { -i } else { i };

        out_idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| i as u8).collect()
    }

    const OK_100_ENCODE_STRS: &[&str] = &[
        "AA==", "AAE=", "AAEC", "AAECAw==", "AAECAwQ=", "AAECAwQF",
        "AAECAwQFBg==", "AAECAwQFBgc=", "AAECAwQFBgcI", "AAECAwQFBgcICQ==",
        "AAECAwQFBgcICQo=", "AAECAwQFBgcICQoL", "AAECAwQFBgcICQoLDA==",
        "AAECAwQFBgcICQoLDA0=", "AAECAwQFBgcICQoLDA0O",
        "AAECAwQFBgcICQoLDA0ODw==", "AAECAwQFBgcICQoLDA0ODxA=",
        "AAECAwQFBgcICQoLDA0ODxAR", "AAECAwQFBgcICQoLDA0ODxAREg==",
        "AAECAwQFBgcICQoLDA0ODxAREhM=", "AAECAwQFBgcICQoLDA0ODxAREhMU",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFQ==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRY=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYX",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGA==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBk=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBka",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGw==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxw=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwd",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHg==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8g",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gIQ==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISI=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIj",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJA==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCU=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUm",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJw==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJyg=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygp",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKg==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKis=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKiss",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLQ==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4v",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMA==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDE=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEy",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMw==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Ng==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OQ==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PA==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+Pw==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0A=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BB",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQg==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkM=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNE",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERQ==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUY=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZH",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSA==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSEk=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElK",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKSw==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0w=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xN",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTg==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk8=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9Q",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUQ==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVI=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJT",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVA==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFU=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVW",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWVw==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1g=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZ",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWg==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWls=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltc",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXQ==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV4=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5f",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYA==",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGE=",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFi",
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFiYw==",
    ];

    #[test]
    fn encode_rfc4648_vectors() {
        let vectors = [
            (&b""[..], ""),
            (&b"f"[..], "Zg=="),
            (&b"fo"[..], "Zm8="),
            (&b"foo"[..], "Zm9v"),
            (&b"foob"[..], "Zm9vYg=="),
            (&b"fooba"[..], "Zm9vYmE="),
            (&b"foobar"[..], "Zm9vYmFy"),
        ];
        for (decoded, encoded) in vectors {
            assert_eq!(base64_encode(decoded), encoded);
        }
    }

    #[test]
    fn decode_rfc4648_vectors() {
        let vectors = [
            ("", &b""[..]),
            ("Zg==", &b"f"[..]),
            ("Zm8=", &b"fo"[..]),
            ("Zm9v", &b"foo"[..]),
            ("Zm9vYg==", &b"foob"[..]),
            ("Zm9vYmE=", &b"fooba"[..]),
            ("Zm9vYmFy", &b"foobar"[..]),
        ];
        for (encoded, decoded) in vectors {
            assert_eq!(base64_decode(encoded), decoded);
        }
    }

    #[test]
    fn encode_empty() {
        assert_eq!(base64_encode(&[]), "");
    }

    #[test]
    fn decode_empty() {
        assert_eq!(base64_decode(""), Vec::<u8>::new());
    }

    #[test]
    fn encode_100_known_values() {
        let data: Vec<u8> = (0..100u8).collect();
        for i in 0..100 {
            assert_eq!(base64_encode(&data[..=i]), OK_100_ENCODE_STRS[i]);
        }
    }

    #[test]
    fn decode_100_known_values() {
        let data: Vec<u8> = (0..100u8).collect();
        for i in 0..100 {
            assert_eq!(base64_decode(OK_100_ENCODE_STRS[i]), &data[..=i]);
        }
    }

    #[test]
    fn encode_decode_roundtrip() {
        let data = generate_data(1024);
        for length in 1..=1024 {
            let encoded = base64_encode(&data[..length]);
            let decoded = base64_decode(&encoded);
            assert_eq!(decoded, &data[..length]);
        }
    }

    #[test]
    fn decode_inplace() {
        let data: Vec<u8> = (0..100u8).collect();
        for i in 0..100 {
            let encoded = base64_encode(&data[..=i]);
            let mut buf = encoded.into_bytes();
            let len = base64_decode_inplace(&mut buf);
            assert_eq!(&buf[..len], &data[..=i]);
        }
    }

    #[test]
    fn incremental_encode_no_break() {
        let data = generate_data(1024);
        let mut encoder = Base64Encoder::new();
        let mut encoded = vec![0u8; 4096];
        let mut total = 0;
        let block_size = 32;
        let mut offset = 0;
        while offset < data.len() {
            let chunk = block_size.min(data.len() - offset);
            total += encoder.encode_step(&data[offset..offset + chunk], false, &mut encoded[total..]);
            offset += chunk;
        }
        total += encoder.encode_close(false, &mut encoded[total..]);
        encoded.truncate(total);
        let expected = base64_encode(&data);
        assert_eq!(encoded, expected.as_bytes());
    }

    #[test]
    fn incremental_encode_with_break() {
        let data = generate_data(1024);
        let mut encoder = Base64Encoder::new();
        let mut encoded = vec![0u8; 4096];
        let mut total = 0;
        let block_size = 32;
        let mut offset = 0;
        while offset < data.len() {
            let chunk = block_size.min(data.len() - offset);
            total += encoder.encode_step(&data[offset..offset + chunk], true, &mut encoded[total..]);
            offset += chunk;
        }
        total += encoder.encode_close(true, &mut encoded[total..]);
        encoded.truncate(total);

        // Decode and verify roundtrip
        let decoded = base64_decode(std::str::from_utf8(&encoded).unwrap());
        assert_eq!(decoded, data);
    }

    #[test]
    fn incremental_decode_small_blocks() {
        let data = generate_data(100);
        for block_size in [1, 2, 3, 4] {
            for i in 0..100 {
                let encoded = base64_encode(&data[..=i]);
                let mut decoder = Base64Decoder::new();
                let mut decoded = vec![0u8; 256];
                let mut total = 0;
                let bytes = encoded.as_bytes();
                let mut offset = 0;
                while offset < bytes.len() {
                    let chunk = block_size.min(bytes.len() - offset);
                    total += decoder.decode_step(&bytes[offset..offset + chunk], &mut decoded[total..]);
                    offset += chunk;
                }
                decoded.truncate(total);
                assert_eq!(decoded, &data[..=i]);
            }
        }
    }

    #[test]
    fn incremental_encode_small_blocks() {
        let data: Vec<u8> = (0..100u8).collect();
        for block_size in [1, 2, 3, 4] {
            for i in 0..100 {
                let encoded_complete = base64_encode(&data[..=i]);

                let mut encoder = Base64Encoder::new();
                let mut encoded_stepped = vec![0u8; 1024];
                let mut total = 0;
                let mut offset = 0;
                while offset <= i {
                    let chunk = block_size.min(i + 1 - offset);
                    total += encoder.encode_step(&data[offset..offset + chunk], false, &mut encoded_stepped[total..]);
                    offset += chunk;
                }
                total += encoder.encode_close(false, &mut encoded_stepped[total..]);
                encoded_stepped.truncate(total);

                assert_eq!(
                    std::str::from_utf8(&encoded_stepped).unwrap(),
                    encoded_complete
                );
                assert_eq!(encoded_complete, OK_100_ENCODE_STRS[i]);
            }
        }
    }
}
