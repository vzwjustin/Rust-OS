//! NLS (Native Language Support) charset and encoding utilities
//!
//! Provides character-set conversion routines used by filesystems such as
//! FAT32, NTFS and ISO9660. Built-in support covers UTF-8, US-ASCII,
//! ISO-8859-1 (Latin-1) and CP437 (the original IBM PC code page). Additional
//! single-byte decoders can be registered at runtime via
//! [`register_charset`], mirroring Linux's `register_nls` interface.

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use lazy_static::lazy_static;
use spin::RwLock;

/// A registered charset decoder: maps a byte stream to Unicode code points.
pub type CharsetDecoder = fn(&[u8]) -> Vec<u32>;

lazy_static! {
    /// Runtime registry of charset decoders, keyed by lowercase charset name.
    static ref CHARSET_REGISTRY: RwLock<BTreeMap<String, CharsetDecoder>> = RwLock::new(BTreeMap::new());
}

/// Normalize a charset name for lookup (lowercase, trim surrounding spaces).
fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

// ---------------------------------------------------------------------------
// Built-in codecs
// ---------------------------------------------------------------------------

/// Encode a Unicode code point as UTF-8.
fn utf8_encode(cp: u32) -> Vec<u8> {
    if cp < 0x80 {
        vec![cp as u8]
    } else if cp < 0x800 {
        vec![0xC0 | ((cp >> 6) as u8 & 0x1F), 0x80 | (cp as u8 & 0x3F)]
    } else if cp < 0x10000 {
        vec![
            0xE0 | ((cp >> 12) as u8 & 0x0F),
            0x80 | ((cp >> 6) as u8 & 0x3F),
            0x80 | (cp as u8 & 0x3F),
        ]
    } else if cp < 0x110000 {
        vec![
            0xF0 | ((cp >> 18) as u8 & 0x07),
            0x80 | ((cp >> 12) as u8 & 0x3F),
            0x80 | ((cp >> 6) as u8 & 0x3F),
            0x80 | (cp as u8 & 0x3F),
        ]
    } else {
        // Encode out-of-range code points as U+FFFD REPLACEMENT CHARACTER.
        utf8_encode(0xFFFD)
    }
}

/// Decode a UTF-8 byte stream into Unicode code points, replacing invalid
/// sequences with U+FFFD.
fn utf8_decode(bytes: &[u8]) -> Vec<u32> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        if b0 < 0x80 {
            out.push(b0 as u32);
            i += 1;
        } else if b0 & 0xE0 == 0xC0 && i + 1 < bytes.len() {
            let b1 = bytes[i + 1];
            if b1 & 0xC0 == 0x80 {
                let cp = ((b0 as u32 & 0x1F) << 6) | (b1 as u32 & 0x3F);
                if cp >= 0x80 {
                    out.push(cp);
                    i += 2;
                    continue;
                }
            }
            out.push(0xFFFD);
            i += 1;
        } else if b0 & 0xF0 == 0xE0 && i + 2 < bytes.len() {
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];
            if b1 & 0xC0 == 0x80 && b2 & 0xC0 == 0x80 {
                let cp =
                    ((b0 as u32 & 0x0F) << 12) | ((b1 as u32 & 0x3F) << 6) | (b2 as u32 & 0x3F);
                if cp >= 0x800 && !(0xD800..=0xDFFF).contains(&cp) {
                    out.push(cp);
                    i += 3;
                    continue;
                }
            }
            out.push(0xFFFD);
            i += 1;
        } else if b0 & 0xF8 == 0xF0 && i + 3 < bytes.len() {
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];
            let b3 = bytes[i + 3];
            if b1 & 0xC0 == 0x80 && b2 & 0xC0 == 0x80 && b3 & 0xC0 == 0x80 {
                let cp = ((b0 as u32 & 0x07) << 18)
                    | ((b1 as u32 & 0x3F) << 12)
                    | ((b2 as u32 & 0x3F) << 6)
                    | (b3 as u32 & 0x3F);
                if cp >= 0x10000 && cp < 0x110000 {
                    out.push(cp);
                    i += 4;
                    continue;
                }
            }
            out.push(0xFFFD);
            i += 1;
        } else {
            out.push(0xFFFD);
            i += 1;
        }
    }
    out
}

/// Decode US-ASCII bytes; bytes outside 0x00..=0x7F become U+FFFD.
fn ascii_decode(bytes: &[u8]) -> Vec<u32> {
    bytes
        .iter()
        .map(|&b| if b < 0x80 { b as u32 } else { 0xFFFD })
        .collect()
}

/// Encode a code point as ASCII, or `?` if it does not fit.
fn ascii_encode(cp: u32) -> Vec<u8> {
    if cp < 0x80 {
        vec![cp as u8]
    } else {
        vec![b'?']
    }
}

/// ISO-8859-1 (Latin-1) is a 1:1 mapping of bytes to U+0000..U+00FF.
fn iso8859_1_decode(bytes: &[u8]) -> Vec<u32> {
    bytes.iter().map(|&b| b as u32).collect()
}

fn iso8859_1_encode(cp: u32) -> Vec<u8> {
    if cp <= 0xFF {
        vec![cp as u8]
    } else {
        vec![b'?']
    }
}

/// CP437 (IBM PC) high-half mapping table (0x80..=0xFF -> Unicode).
const CP437_HIGH: &[u32] = &[
    0x00C7, 0x00FC, 0x00E9, 0x00E2, 0x00E4, 0x00E0, 0x00E5, 0x00E7, // 80-87
    0x00EA, 0x00EB, 0x00E8, 0x00EF, 0x00EE, 0x00EC, 0x00C4, 0x00C5, // 88-8F
    0x00C9, 0x00E6, 0x00C6, 0x00F4, 0x00F6, 0x00F2, 0x00FB, 0x00F9, // 90-97
    0x00FF, 0x00D6, 0x00DC, 0x00A2, 0x00A3, 0x00A5, 0x20A7, 0x0192, // 98-9F
    0x00E1, 0x00ED, 0x00F3, 0x00FA, 0x00F1, 0x00D1, 0x00AA, 0x00BA, // A0-A7
    0x00BF, 0x2310, 0x00AC, 0x00BD, 0x00BC, 0x00A1, 0x00AB, 0x00BB, // A8-AF
    0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x2561, 0x2562, 0x2556, // B0-B7
    0x2555, 0x2563, 0x2551, 0x2557, 0x255D, 0x255C, 0x255B, 0x2510, // B8-BF
    0x2514, 0x2534, 0x252C, 0x251C, 0x2500, 0x253C, 0x255E, 0x255F, // C0-C7
    0x255A, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256C, 0x2567, // C8-CF
    0x2568, 0x2564, 0x2565, 0x2552, 0x2553, 0x256B, 0x256A, 0x2518, // D0-D7
    0x250C, 0x2588, 0x2584, 0x258C, 0x2590, 0x2580, 0x03B1, 0x00DF, // D8-DF
    0x0393, 0x03C0, 0x03A3, 0x03C3, 0x00B5, 0x03C4, 0x03A6, 0x0398, // E0-E7
    0x03A9, 0x03B4, 0x221E, 0x03C6, 0x03B5, 0x2229, 0x2261, 0x00B1, // E8-EF
    0x2265, 0x2264, 0x2320, 0x2321, 0x00F7, 0x2248, 0x00B0, 0x2219, // F0-F7
    0x00B7, 0x221A, 0x207F, 0x00B2, 0x25A0, 0x00A0, // F8-FD
];

fn cp437_decode(bytes: &[u8]) -> Vec<u32> {
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                b as u32
            } else {
                CP437_HIGH[(b - 0x80) as usize]
            }
        })
        .collect()
}

fn cp437_encode(cp: u32) -> Vec<u8> {
    if cp < 0x80 {
        return vec![cp as u8];
    }
    for (i, &u) in CP437_HIGH.iter().enumerate() {
        if u == cp {
            return vec![(0x80 + i) as u8];
        }
    }
    vec![b'?']
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Convert a Unicode character to a specific charset
///
/// # Arguments
/// * `char_code` - Unicode code point to convert
/// * `charset` - Target character set name (e.g., "utf-8", "iso8859-1")
///
/// # Returns
/// Byte sequence representing the character in the target charset
pub fn unicode_to_charset(char_code: u32, charset: &str) -> Vec<u8> {
    match normalize_name(charset).as_str() {
        "utf-8" | "utf8" => utf8_encode(char_code),
        "ascii" | "us-ascii" => ascii_encode(char_code),
        "iso8859-1" | "iso-8859-1" | "latin-1" | "latin1" => iso8859_1_encode(char_code),
        "cp437" | "ibm437" => cp437_encode(char_code),
        // Unknown/registered charsets: fall back to UTF-8 so no data is lost.
        _ => utf8_encode(char_code),
    }
}

/// Convert a charset-encoded string to Unicode
///
/// # Arguments
/// * `bytes` - Bytes in the source charset
/// * `charset` - Source character set name
///
/// # Returns
/// Vector of Unicode code points
pub fn charset_to_unicode(bytes: &[u8], charset: &str) -> Vec<u32> {
    let name = normalize_name(charset);
    match name.as_str() {
        "utf-8" | "utf8" => return utf8_decode(bytes),
        "ascii" | "us-ascii" => return ascii_decode(bytes),
        "iso8859-1" | "iso-8859-1" | "latin-1" | "latin1" => return iso8859_1_decode(bytes),
        "cp437" | "ibm437" => return cp437_decode(bytes),
        _ => {}
    }
    // Consult the runtime registry for custom charsets.
    if let Some(decoder) = CHARSET_REGISTRY.read().get(&name).copied() {
        return decoder(bytes);
    }
    // Default: treat as Latin-1 (lossless for single bytes).
    iso8859_1_decode(bytes)
}

/// Register a character set converter
///
/// # Arguments
/// * `name` - Character set name
/// * `converter` - Function pointer for decoding bytes to code points
pub fn register_charset(name: &str, converter: fn(&[u8]) -> Vec<u32>) -> Result<(), String> {
    let key = normalize_name(name);
    if key.is_empty() {
        return Err("empty charset name".to_string());
    }
    let mut registry = CHARSET_REGISTRY.write();
    if registry.contains_key(&key) {
        return Err(format!("charset '{}' already registered", key));
    }
    registry.insert(key, converter);
    Ok(())
}

/// Unregister a character set converter
pub fn unregister_charset(name: &str) -> Result<(), String> {
    let key = normalize_name(name);
    let mut registry = CHARSET_REGISTRY.write();
    if registry.remove(&key).is_some() {
        Ok(())
    } else {
        Err(format!("charset '{}' not registered", key))
    }
}
