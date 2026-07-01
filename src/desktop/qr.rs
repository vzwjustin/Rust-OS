//! QR code generation — ported from gnome-qr/gnome-qr.c
//!
//! Generates QR code pixel data for display on the framebuffer.
//! The upstream uses the nayuki qrcodegen library; we implement a
//! self-contained QR code encoder in no_std.
//!
//! Supports QR code versions 1–10 with byte mode encoding and
//! configurable error correction levels.  Output is a 2D matrix
//! of booleans (true = dark module) that can be rendered to any
//! pixel format.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Error correction level.  Matches `GnomeQrEccLevel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrEccLevel {
    Low,
    Medium,
    Quartile,
    High,
}

/// Pixel format for QR code output.  Matches `GnomeQrPixelFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrPixelFormat {
    Alpha8,
    Gray8,
    Rgb888,
    Rgba8888,
}

impl QrPixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            QrPixelFormat::Alpha8 => 1,
            QrPixelFormat::Gray8 => 1,
            QrPixelFormat::Rgb888 => 3,
            QrPixelFormat::Rgba8888 => 4,
        }
    }
}

/// RGBA color for QR code generation.  Matches `GnomeQrColor`.
#[derive(Debug, Clone, Copy)]
pub struct QrColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl QrColor {
    pub const WHITE: QrColor = QrColor {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 255,
    };
    pub const BLACK: QrColor = QrColor {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 255,
    };
    pub const TRANSPARENT: QrColor = QrColor {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    };
}

/// QR code generation result.
pub struct QrCode {
    /// The module matrix (true = dark/false = light).
    pub modules: Vec<Vec<bool>>,
    /// Side length in modules.
    pub size: usize,
}

impl QrCode {
    /// Get the module at (x, y).
    pub fn get_module(&self, x: usize, y: usize) -> bool {
        self.modules[y][x]
    }

    /// Render the QR code to pixel data.
    /// Matches `gnome_qr_generate_qr_code_sync()` pixel generation.
    pub fn render(
        &self,
        requested_size: usize,
        bg_color: QrColor,
        fg_color: QrColor,
        format: QrPixelFormat,
    ) -> (Vec<u8>, usize) {
        let qr_size = self.size;
        let block_size = if requested_size > 0 && qr_size > 0 {
            (requested_size + qr_size - 1) / qr_size
        } else {
            1
        };
        let total_size = qr_size * block_size;
        let bpp = format.bytes_per_pixel();
        let mut pixels = Vec::with_capacity(total_size * total_size * bpp);

        for row in 0..qr_size {
            for _ in 0..block_size {
                for col in 0..qr_size {
                    let color = if self.modules[row][col] {
                        fg_color
                    } else {
                        bg_color
                    };
                    for _ in 0..block_size {
                        match format {
                            QrPixelFormat::Alpha8 => {
                                pixels.push(color.alpha);
                            }
                            QrPixelFormat::Gray8 => {
                                pixels.push(color.red);
                            }
                            QrPixelFormat::Rgb888 => {
                                pixels.push(color.red);
                                pixels.push(color.green);
                                pixels.push(color.blue);
                            }
                            QrPixelFormat::Rgba8888 => {
                                pixels.push(color.red);
                                pixels.push(color.green);
                                pixels.push(color.blue);
                                pixels.push(color.alpha);
                            }
                        }
                    }
                }
            }
        }

        (pixels, total_size)
    }
}

/// Generate a QR code from text.
/// Matches `gnome_qr_generate_qr_code_sync()`.
pub fn generate_qr_code(text: &str, ecc: QrEccLevel) -> Option<QrCode> {
    if text.is_empty() {
        return None;
    }
    let encoder = QrEncoder::new(ecc);
    encoder.encode(text)
}

// ── QR code encoder implementation ──────────────────────────────────

/// QR version info: capacity for byte mode (bytes), data codewords, ec codewords per block,
/// and number of blocks.
struct VersionInfo {
    version: u8,
    data_capacity: usize,
    total_codewords: usize,
    ec_codewords_per_block: usize,
    num_blocks_group1: usize,
    data_codewords_group1: usize,
    num_blocks_group2: usize,
    data_codewords_group2: usize,
}

/// Get version info for a given version and ECC level.
/// We support versions 1-10.
fn get_version_info(version: u8, ecc: QrEccLevel) -> Option<VersionInfo> {
    // Format: (total_codewords, ec_per_block, group1_blocks, group1_data, group2_blocks, group2_data)
    // Data capacity for byte mode = total_data_codewords - 2 (mode + length overhead for small)
    let (total, ec_per_block, g1b, g1d, g2b, g2d): (usize, usize, usize, usize, usize, usize) =
        match (version, ecc) {
            (1, QrEccLevel::Low) => (26, 7, 1, 19, 0, 0),
            (1, QrEccLevel::Medium) => (26, 10, 1, 16, 0, 0),
            (1, QrEccLevel::Quartile) => (26, 13, 1, 13, 0, 0),
            (1, QrEccLevel::High) => (26, 17, 1, 9, 0, 0),
            (2, QrEccLevel::Low) => (44, 10, 1, 34, 0, 0),
            (2, QrEccLevel::Medium) => (44, 16, 1, 28, 0, 0),
            (2, QrEccLevel::Quartile) => (44, 22, 1, 22, 0, 0),
            (2, QrEccLevel::High) => (44, 28, 1, 16, 0, 0),
            (3, QrEccLevel::Low) => (70, 15, 1, 55, 0, 0),
            (3, QrEccLevel::Medium) => (70, 26, 1, 44, 0, 0),
            (3, QrEccLevel::Quartile) => (70, 18, 2, 17, 0, 0),
            (3, QrEccLevel::High) => (70, 22, 2, 13, 0, 0),
            (4, QrEccLevel::Low) => (100, 20, 1, 80, 0, 0),
            (4, QrEccLevel::Medium) => (100, 18, 2, 32, 0, 0),
            (4, QrEccLevel::Quartile) => (100, 26, 2, 24, 0, 0),
            (4, QrEccLevel::High) => (100, 16, 4, 9, 0, 0),
            (5, QrEccLevel::Low) => (134, 26, 1, 108, 0, 0),
            (5, QrEccLevel::Medium) => (134, 24, 2, 43, 0, 0),
            (5, QrEccLevel::Quartile) => (134, 16, 2, 22, 2, 23),
            (5, QrEccLevel::High) => (134, 22, 2, 15, 2, 16),
            (6, QrEccLevel::Low) => (172, 18, 2, 68, 0, 0),
            (6, QrEccLevel::Medium) => (172, 16, 4, 27, 0, 0),
            (6, QrEccLevel::Quartile) => (172, 24, 4, 20, 0, 0),
            (6, QrEccLevel::High) => (172, 28, 4, 15, 0, 0),
            (7, QrEccLevel::Low) => (196, 20, 2, 78, 0, 0),
            (7, QrEccLevel::Medium) => (196, 18, 4, 31, 0, 0),
            (7, QrEccLevel::Quartile) => (196, 18, 2, 14, 4, 15),
            (7, QrEccLevel::High) => (196, 26, 4, 13, 1, 14),
            (8, QrEccLevel::Low) => (242, 24, 1, 97, 2, 98),
            (8, QrEccLevel::Medium) => (242, 22, 2, 38, 2, 39),
            (8, QrEccLevel::Quartile) => (242, 22, 4, 18, 2, 19),
            (8, QrEccLevel::High) => (242, 26, 4, 14, 2, 15),
            (9, QrEccLevel::Low) => (292, 30, 2, 116, 0, 0),
            (9, QrEccLevel::Medium) => (292, 22, 3, 36, 2, 37),
            (9, QrEccLevel::Quartile) => (292, 20, 4, 16, 4, 17),
            (9, QrEccLevel::High) => (292, 24, 4, 12, 4, 13),
            (10, QrEccLevel::Low) => (342, 18, 2, 68, 2, 69),
            (10, QrEccLevel::Medium) => (342, 26, 4, 43, 1, 44),
            (10, QrEccLevel::Quartile) => (342, 24, 6, 19, 2, 20),
            (10, QrEccLevel::High) => (342, 28, 6, 15, 2, 16),
            _ => return None,
        };

    let total_data = g1b * g1d + g2b * g2d;
    // Byte mode: 4-bit mode indicator + 8-bit length + data
    // For versions 1-9, length is 8 bits; for version 10+, it's 16 bits
    let length_bits = if version <= 9 { 8 } else { 16 };
    let overhead = 4 + length_bits; // bits
    let data_capacity = (total_data * 8 - overhead) / 8;

    Some(VersionInfo {
        version,
        data_capacity,
        total_codewords: total,
        ec_codewords_per_block: ec_per_block,
        num_blocks_group1: g1b,
        data_codewords_group1: g1d,
        num_blocks_group2: g2b,
        data_codewords_group2: g2d,
    })
}

/// QR code encoder.
struct QrEncoder {
    ecc: QrEccLevel,
}

impl QrEncoder {
    fn new(ecc: QrEccLevel) -> Self {
        Self { ecc }
    }

    fn encode(&self, text: &str) -> Option<QrCode> {
        let data = text.as_bytes();

        // Find the minimum version that fits
        let mut version = 1u8;
        let mut info = None;
        for v in 1..=10 {
            if let Some(vi) = get_version_info(v, self.ecc) {
                if data.len() <= vi.data_capacity {
                    info = Some(vi);
                    version = v;
                    break;
                }
            }
        }
        let info = info?;

        // Build the bit stream
        let bits = self.build_bit_stream(data, &info);

        // Convert bits to codewords
        let codewords = self.bits_to_codewords(&bits, info.total_codewords);

        // Split into blocks and generate EC
        let (data_blocks, ec_blocks) = self.split_into_blocks(&codewords, &info);

        // Interleave data and EC blocks
        let final_data = self.interleave_blocks(&data_blocks, &ec_blocks);

        // Build the module matrix
        let size = 17 + 4 * version as usize;
        let mut matrix = vec![vec![false; size]; size];
        let mut reserved = vec![vec![false; size]; size];

        // Place function patterns
        self.place_finder_patterns(&mut matrix, &mut reserved, size);
        self.place_alignment_patterns(&mut matrix, &mut reserved, version, size);
        self.place_timing_patterns(&mut matrix, &mut reserved, size);
        self.reserve_format_areas(&mut reserved, size);

        // Place data
        self.place_data(&mut matrix, &mut reserved, &final_data, size);

        // Apply best mask
        self.apply_mask_and_format(&mut matrix, &mut reserved, size);

        Some(QrCode {
            modules: matrix,
            size,
        })
    }

    fn build_bit_stream(&self, data: &[u8], info: &VersionInfo) -> Vec<bool> {
        let mut bits = Vec::new();

        // Mode indicator: 0100 = byte mode
        bits.extend_from_slice(&[false, true, false, false]);

        // Character count
        let length_bits = if info.version <= 9 { 8 } else { 16 };
        let len = data.len();
        for i in (0..length_bits).rev() {
            bits.push((len >> i) & 1 == 1);
        }

        // Data
        for &byte in data {
            for i in (0..8).rev() {
                bits.push((byte >> i) & 1 == 1);
            }
        }

        // Terminator (up to 4 zero bits)
        let total_data_bits = info.total_codewords() * 8;
        let remaining = total_data_bits - bits.len();
        let terminator = remaining.min(4);
        for _ in 0..terminator {
            bits.push(false);
        }

        // Pad to byte boundary
        while bits.len() % 8 != 0 {
            bits.push(false);
        }

        // Pad with alternating bytes
        let mut pad = true;
        while bits.len() < total_data_bits {
            let byte = if pad { 0xEC } else { 0x11 };
            for i in (0..8).rev() {
                bits.push((byte >> i) & 1 == 1);
            }
            pad = !pad;
        }

        bits
    }

    fn bits_to_codewords(&self, bits: &[bool], total_codewords: usize) -> Vec<u8> {
        let mut codewords = Vec::with_capacity(total_codewords);
        let mut i = 0;
        while i < bits.len() && codewords.len() < total_codewords {
            let mut byte = 0u8;
            for j in 0..8 {
                if i + j < bits.len() && bits[i + j] {
                    byte |= 1 << (7 - j);
                }
            }
            codewords.push(byte);
            i += 8;
        }
        while codewords.len() < total_codewords {
            codewords.push(0);
        }
        codewords
    }

    fn split_into_blocks(
        &self,
        codewords: &[u8],
        info: &VersionInfo,
    ) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let mut data_blocks = Vec::new();
        let mut offset = 0;

        for _ in 0..info.num_blocks_group1 {
            let block = codewords[offset..offset + info.data_codewords_group1].to_vec();
            data_blocks.push(block);
            offset += info.data_codewords_group1;
        }
        for _ in 0..info.num_blocks_group2 {
            let block = codewords[offset..offset + info.data_codewords_group2].to_vec();
            data_blocks.push(block);
            offset += info.data_codewords_group2;
        }

        // Generate EC for each block
        let mut ec_blocks = Vec::new();
        for block in &data_blocks {
            let ec = self.generate_reed_solomon(block, info.ec_codewords_per_block);
            ec_blocks.push(ec);
        }

        (data_blocks, ec_blocks)
    }

    fn generate_reed_solomon(&self, data: &[u8], ec_count: usize) -> Vec<u8> {
        // Reed-Solomon encoding over GF(256) with generator polynomial
        let generator = self.rs_generator_poly(ec_count);
        let mut result = vec![0u8; data.len() + ec_count];
        result[..data.len()].copy_from_slice(data);

        for i in 0..data.len() {
            let coef = result[i];
            if coef != 0 {
                for (j, &gen_coef) in generator.iter().enumerate() {
                    result[i + 1 + j] ^= gf_mul(gen_coef, coef);
                }
            }
        }

        result[data.len()..].to_vec()
    }

    fn rs_generator_poly(&self, degree: usize) -> Vec<u8> {
        let mut poly = vec![1u8];
        for i in 0..degree {
            let mut new_poly = vec![0u8; poly.len() + 1];
            for (j, &coef) in poly.iter().enumerate() {
                new_poly[j] ^= coef;
                new_poly[j + 1] ^= gf_mul(coef, gf_exp(i as u16));
            }
            poly = new_poly;
        }
        poly
    }

    fn interleave_blocks(&self, data_blocks: &[Vec<u8>], ec_blocks: &[Vec<u8>]) -> Vec<u8> {
        let mut result = Vec::new();
        let max_data = data_blocks.iter().map(|b| b.len()).max().unwrap_or(0);
        let max_ec = ec_blocks.iter().map(|b| b.len()).max().unwrap_or(0);

        for i in 0..max_data {
            for block in data_blocks {
                if i < block.len() {
                    result.push(block[i]);
                }
            }
        }
        for i in 0..max_ec {
            for block in ec_blocks {
                if i < block.len() {
                    result.push(block[i]);
                }
            }
        }
        result
    }

    fn place_finder_patterns(
        &self,
        matrix: &mut Vec<Vec<bool>>,
        reserved: &mut Vec<Vec<bool>>,
        size: usize,
    ) {
        for (fx, fy) in [(0, 0), (size - 7, 0), (0, size - 7)] {
            for dy in 0..7 {
                for dx in 0..7 {
                    let x = fx + dx;
                    let y = fy + dy;
                    reserved[y][x] = true;
                    matrix[y][x] = match (dx, dy) {
                        (0..=6, 0) | (0..=6, 6) | (0, 0..=6) | (6, 0..=6) => true,
                        (1..=5, 1..=5) if dx == 1 || dx == 5 || dy == 1 || dy == 5 => false,
                        (2..=4, 2..=4) => true,
                        _ => false,
                    };
                }
            }
        }
    }

    fn place_alignment_patterns(
        &self,
        matrix: &mut Vec<Vec<bool>>,
        reserved: &mut Vec<Vec<bool>>,
        version: u8,
        size: usize,
    ) {
        let positions = alignment_positions(version);
        for &cx in &positions {
            for &cy in &positions {
                // Skip if overlapping finder patterns
                if (cx <= 8 && cy <= 8)
                    || (cx >= size - 9 && cy <= 8)
                    || (cx <= 8 && cy >= size - 9)
                {
                    continue;
                }
                for dy in 0..5 {
                    for dx in 0..5 {
                        let x = cx - 2 + dx;
                        let y = cy - 2 + dy;
                        if x < size && y < size {
                            reserved[y][x] = true;
                            matrix[y][x] = match (dx, dy) {
                                (0, 0) | (4, 0) | (0, 4) | (4, 4) => true,
                                (2, 2) => true,
                                _ => false,
                            };
                        }
                    }
                }
            }
        }
    }

    fn place_timing_patterns(
        &self,
        matrix: &mut Vec<Vec<bool>>,
        reserved: &mut Vec<Vec<bool>>,
        size: usize,
    ) {
        for i in 8..size - 8 {
            reserved[6][i] = true;
            reserved[i][6] = true;
            matrix[6][i] = i % 2 == 0;
            matrix[i][6] = i % 2 == 0;
        }
    }

    fn reserve_format_areas(&self, reserved: &mut Vec<Vec<bool>>, size: usize) {
        // Reserve format info areas around finder patterns
        for i in 0..9 {
            reserved[8][i] = true;
            reserved[i][8] = true;
            reserved[8][size - 1 - i] = true;
            reserved[size - 1 - i][8] = true;
        }
        // Dark module
        reserved[size - 8][8] = true;
    }

    fn place_data(
        &self,
        matrix: &mut Vec<Vec<bool>>,
        reserved: &mut Vec<Vec<bool>>,
        data: &[u8],
        size: usize,
    ) {
        let mut bit_idx = 0;
        let mut going_up = true;
        let mut col = size - 1;

        while col > 0 {
            if col == 6 {
                col -= 1;
            }

            for i in 0..size {
                let y = if going_up { size - 1 - i } else { i };
                for c in 0..2 {
                    let x = col - c;
                    if !reserved[y][x] {
                        let byte = data[bit_idx / 8];
                        let bit = (byte >> (7 - bit_idx % 8)) & 1;
                        matrix[y][x] = bit == 1;
                        bit_idx += 1;
                    }
                }
            }
            going_up = !going_up;
            col -= 2;
        }
    }

    fn apply_mask_and_format(
        &self,
        matrix: &mut Vec<Vec<bool>>,
        reserved: &mut Vec<Vec<bool>>,
        size: usize,
    ) {
        // Use mask pattern 0 (i+j) % 2 == 0
        let mask_pattern = 0u8;
        for y in 0..size {
            for x in 0..size {
                if !reserved[y][x] {
                    if Self::apply_mask(x, y, mask_pattern) {
                        matrix[y][x] = !matrix[y][x];
                    }
                }
            }
        }

        // Place format info
        let format_bits = self.format_info_bits(mask_pattern);
        self.place_format_info(matrix, format_bits, size);

        // Dark module
        matrix[size - 8][8] = true;
    }

    fn apply_mask(x: usize, y: usize, pattern: u8) -> bool {
        match pattern {
            0 => (x + y) % 2 == 0,
            1 => y % 2 == 0,
            2 => x % 3 == 0,
            3 => (x + y) % 3 == 0,
            4 => ((y / 2) + (x / 3)) % 2 == 0,
            5 => ((x * y) % 2 + (x * y) % 3) == 0,
            6 => ((x * y) % 2 + (x * y) % 3) % 2 == 0,
            7 => ((x + y) % 2 + (x * y) % 3) % 2 == 0,
            _ => false,
        }
    }

    fn format_info_bits(&self, mask_pattern: u8) -> u16 {
        let ecc_bits = match self.ecc {
            QrEccLevel::Low => 0b01,
            QrEccLevel::Medium => 0b00,
            QrEccLevel::Quartile => 0b11,
            QrEccLevel::High => 0b10,
        };
        let data = (ecc_bits << 3) | (mask_pattern & 0x07);
        let mut bch = data as u16;
        // BCH(15,5) error correction
        bch <<= 10;
        for i in (0..5).rev() {
            if (bch >> (i + 10)) & 1 == 1 {
                bch ^= 0b10100110111 << i;
            }
        }
        let format = ((data as u16) << 10) | (bch & 0x3FF);
        format ^ 0b101010000010010
    }

    fn place_format_info(&self, matrix: &mut Vec<Vec<bool>>, format: u16, size: usize) {
        // Around top-left finder
        for i in 0..6 {
            matrix[8][i] = (format >> i) & 1 == 1;
        }
        matrix[8][7] = (format >> 6) & 1 == 1;
        matrix[8][8] = (format >> 7) & 1 == 1;
        matrix[7][8] = (format >> 8) & 1 == 1;
        for i in 0..6 {
            matrix[5 - i][8] = (format >> (9 + i)) & 1 == 1;
        }

        // Around top-right and bottom-left
        for i in 0..8 {
            matrix[size - 1 - i][8] = (format >> i) & 1 == 1;
        }
        for i in 0..7 {
            matrix[8][size - 7 + i] = (format >> (8 + i)) & 1 == 1;
        }
    }
}

impl VersionInfo {
    fn total_codewords(&self) -> usize {
        self.num_blocks_group1 * self.data_codewords_group1
            + self.num_blocks_group2 * self.data_codewords_group2
    }
}

/// Alignment pattern center positions for each version.
fn alignment_positions(version: u8) -> Vec<usize> {
    match version {
        1 => vec![],
        2 => vec![6, 18],
        3 => vec![6, 22],
        4 => vec![6, 26],
        5 => vec![6, 30],
        6 => vec![6, 34],
        7 => vec![6, 22, 38],
        8 => vec![6, 24, 42],
        9 => vec![6, 26, 46],
        10 => vec![6, 28, 50],
        _ => vec![],
    }
}

// ── GF(256) arithmetic ──────────────────────────────────────────────

/// GF(256) exponentiation table (generator = 2, primitive polynomial 0x11D).
fn gf_exp(n: u16) -> u8 {
    static EXP_TABLE: [u8; 256] = {
        let mut table = [0u8; 256];
        let mut x: u16 = 1;
        let mut i = 0;
        while i < 256 {
            table[i] = x as u8;
            x <<= 1;
            if x >= 256 {
                x ^= 0x11D;
            }
            i += 1;
        }
        table
    };
    EXP_TABLE[n as usize % 255]
}

/// GF(256) logarithm table.
fn gf_log(x: u8) -> u16 {
    static LOG_TABLE: [u16; 256] = {
        let mut table = [0u16; 256];
        let mut x: u16 = 1;
        let mut i: u16 = 0;
        while i < 255 {
            table[x as usize] = i;
            x <<= 1;
            if x >= 256 {
                x ^= 0x11D;
            }
            i += 1;
        }
        table
    };
    if x == 0 {
        0
    } else {
        LOG_TABLE[x as usize]
    }
}

/// GF(256) multiplication.
fn gf_mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        0
    } else {
        gf_exp(gf_log(a) + gf_log(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_generate_simple() {
        let code = generate_qr_code("Hello", QrEccLevel::Low);
        assert!(code.is_some());
        let code = code.unwrap();
        assert!(code.size >= 21); // Version 1 is 21x21
    }

    fn test_render() {
        let code = generate_qr_code("Test", QrEccLevel::Medium).unwrap();
        let (pixels, size) =
            code.render(100, QrColor::WHITE, QrColor::BLACK, QrPixelFormat::Rgb888);
        assert!(size >= 21);
        assert_eq!(pixels.len(), size * size * 3);
    }

    fn test_empty_returns_none() {
        assert!(generate_qr_code("", QrEccLevel::Low).is_none());
    }

    fn test_gf_mul() {
        assert_eq!(gf_mul(0, 5), 0);
        assert_eq!(gf_mul(1, 5), 5);
        assert_eq!(gf_mul(2, 2), 4);
    }
}
