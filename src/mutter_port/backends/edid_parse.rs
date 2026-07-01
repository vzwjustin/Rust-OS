//! EDID parsing ported from GNOME Mutter's src/backends/edid-parse.c (+ edid.h)
//!
//! Provides `EdidInfo` (mirrors MetaEdidInfo) and `EdidInfo::parse`, which
//! reads the identifying fields out of a raw EDID base block. Upstream mutter
//! delegates the byte parsing to the external `libdisplay-info` library; since
//! that library is unavailable in the kernel, the classic 128-byte EDID 1.x
//! base-block parse is implemented here directly (pure computation).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/edid-parse.c

use alloc::string::String;

/// CIE 1931 xy chromaticity coordinates, mirrors di_color_primaries.
#[derive(Debug, Clone, Copy, Default)]
pub struct ColorPrimaries {
    pub has_primaries: bool,
    pub has_default_white_point: bool,
    pub primary: [(f32, f32); 3],
    pub default_white: (f32, f32),
}

/// Parsed EDID identification, mirrors MetaEdidInfo.
#[derive(Debug, Clone, Default)]
pub struct EdidInfo {
    /// Three-letter PnP manufacturer code.
    pub manufacturer_code: String,
    pub product_code: i32,
    pub serial_number: u32,

    /// Optional product description strings from display descriptors.
    pub dsc_serial_number: Option<String>,
    pub dsc_product_name: Option<String>,

    pub default_color_primaries: ColorPrimaries,
    /// -1.0 if not specified.
    pub default_gamma: f64,

    pub min_vert_rate_hz: i32,
}

impl EdidInfo {
    /// meta_edid_info_new_parse(): parse a raw EDID blob. Returns None if the
    /// blob is too short or the header magic is wrong.
    pub fn parse(edid: &[u8]) -> Option<EdidInfo> {
        if edid.len() < 128 {
            return None;
        }
        // EDID base-block header magic: 00 FF FF FF FF FF FF 00.
        const MAGIC: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
        if edid[0..8] != MAGIC {
            return None;
        }

        let mut info = EdidInfo {
            default_gamma: -1.0,
            ..EdidInfo::default()
        };

        // Vendor / product identification (bytes 8..18).
        info.manufacturer_code = decode_manufacturer(edid[8], edid[9]);
        info.product_code = (edid[10] as i32) | ((edid[11] as i32) << 8);
        info.serial_number = u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]]);

        // Gamma (byte 23): stored as (gamma * 100) - 100; 0xFF means unspecified.
        if edid[23] != 0xFF {
            info.default_gamma = (edid[23] as f64 + 100.0) / 100.0;
        }

        // Chromaticity coordinates (bytes 25..35): 10-bit values.
        info.default_color_primaries = decode_chromaticity(&edid[25..35]);

        // Four 18-byte detailed timing / display descriptors at 54, 72, 90, 108.
        for base in [54usize, 72, 90, 108] {
            let d = &edid[base..base + 18];
            // A descriptor (not a detailed timing) has bytes 0-1 == 0.
            if d[0] != 0 || d[1] != 0 {
                continue;
            }
            match d[3] {
                // 0xFF: display product serial number.
                0xFF => info.dsc_serial_number = decode_descriptor_string(&d[5..18]),
                // 0xFC: display product name.
                0xFC => info.dsc_product_name = decode_descriptor_string(&d[5..18]),
                // 0xFD: display range limits; byte 5 = min vertical rate (Hz).
                0xFD => info.min_vert_rate_hz = d[5] as i32,
                _ => {}
            }
        }

        Some(info)
    }
}

/// Decode the packed 5-bit-per-letter PnP manufacturer id (bytes 8-9).
fn decode_manufacturer(b0: u8, b1: u8) -> String {
    let packed = ((b0 as u16) << 8) | (b1 as u16);
    let mut s = String::new();
    for shift in [10u16, 5, 0] {
        let v = ((packed >> shift) & 0x1f) as u8;
        s.push((b'A' + v - 1) as char);
    }
    s
}

/// Decode a display-descriptor text field, trimming at LF / trailing spaces.
fn decode_descriptor_string(bytes: &[u8]) -> Option<String> {
    let mut s = String::new();
    for &b in bytes {
        if b == 0x0A {
            break;
        }
        s.push(b as char);
    }
    let trimmed = s.trim_end();
    if trimmed.is_empty() {
        None
    } else {
        Some(String::from(trimmed))
    }
}

/// Decode the 10 chromaticity bytes into red/green/blue + white points.
fn decode_chromaticity(b: &[u8]) -> ColorPrimaries {
    // Low 2 bits of each 10-bit value are packed into bytes 0-1.
    let rx = ((b[2] as u16) << 2) | (((b[0] >> 6) & 0x3) as u16);
    let ry = ((b[3] as u16) << 2) | (((b[0] >> 4) & 0x3) as u16);
    let gx = ((b[4] as u16) << 2) | (((b[0] >> 2) & 0x3) as u16);
    let gy = ((b[5] as u16) << 2) | (((b[0]) & 0x3) as u16);
    let bx = ((b[6] as u16) << 2) | (((b[1] >> 6) & 0x3) as u16);
    let by = ((b[7] as u16) << 2) | (((b[1] >> 4) & 0x3) as u16);
    let wx = ((b[8] as u16) << 2) | (((b[1] >> 2) & 0x3) as u16);
    let wy = ((b[9] as u16) << 2) | (((b[1]) & 0x3) as u16);

    let f = |v: u16| (v as f32) / 1024.0;
    ColorPrimaries {
        has_primaries: true,
        has_default_white_point: true,
        primary: [(f(rx), f(ry)), (f(gx), f(gy)), (f(bx), f(by))],
        default_white: (f(wx), f(wy)),
    }
}
