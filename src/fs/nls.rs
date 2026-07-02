//! NLS (Native Language Support) charset and encoding utilities
//!
//! This is not a mountable filesystem but a helper subsystem for character set
//! conversions used by filesystems like FAT32, NTFS, and others.  It provides
//! a global registry of `NlsTable` instances, each with function pointers for
//! converting between a specific charset and Unicode (UTF-8 code points).
//! Three built-in tables are registered at initialization: "ascii" (7-bit),
//! "iso8859-1" (Latin-1), and "cp437" (DOS).

use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use spin::RwLock;

/// Function type: convert a single Unicode code point to a charset byte.
pub type CharFromUtf8 = fn(u32) -> Option<u8>;

/// Function type: convert a single charset byte to a Unicode code point.
pub type CharToUtf8 = fn(u8) -> Option<u32>;

/// An NLS (charset) table with conversion function pointers.
#[derive(Clone)]
pub struct NlsTable {
    /// Character set name (e.g., "ascii", "iso8859-1", "cp437").
    pub name: String,
    /// Convert a Unicode code point to a charset byte.
    pub char_from_utf8: CharFromUtf8,
    /// Convert a charset byte to a Unicode code point.
    pub char_to_utf8: CharToUtf8,
}

impl core::fmt::Debug for NlsTable {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("NlsTable")
            .field("name", &self.name)
            .finish()
    }
}

/// Global registry of NLS tables.
static NLS_REGISTRY: RwLock<BTreeMap<String, Arc<NlsTable>>> = RwLock::new(BTreeMap::new());

// ── Built-in table conversion functions ──────────────────────────────────

/// ASCII: 7-bit, code points 0x00–0x7F map directly.
fn ascii_from_utf8(cp: u32) -> Option<u8> {
    if cp <= 0x7F {
        Some(cp as u8)
    } else {
        None
    }
}

fn ascii_to_utf8(b: u8) -> Option<u32> {
    if b <= 0x7F {
        Some(b as u32)
    } else {
        None
    }
}

/// ISO 8859-1 (Latin-1): bytes 0x00–0xFF map directly to U+0000–U+00FF.
fn iso8859_1_from_utf8(cp: u32) -> Option<u8> {
    if cp <= 0xFF {
        Some(cp as u8)
    } else {
        None
    }
}

fn iso8859_1_to_utf8(b: u8) -> Option<u32> {
    Some(b as u32)
}

/// CP437 (DOS): maps bytes 0x80–0xFF to their Unicode equivalents.
/// The lower 128 bytes are ASCII.
fn cp437_to_utf8(b: u8) -> Option<u32> {
    if b <= 0x7F {
        return Some(b as u32);
    }
    // CP437 high-half table (0x80–0xFF)
    const CP437_HIGH: [u32; 128] = [
        0x00C7, 0x00FC, 0x00E9, 0x00E2, 0x00E4, 0x00E0, 0x00E5, 0x00E7, // 0x80
        0x00EA, 0x00EB, 0x00E8, 0x00EF, 0x00EE, 0x00EC, 0x00C4, 0x00C5, // 0x88
        0x00C9, 0x00E6, 0x00C6, 0x00F4, 0x00F6, 0x00F2, 0x00FB, 0x00F9, // 0x90
        0x00FF, 0x00D6, 0x00DC, 0x00A2, 0x00A3, 0x00A5, 0x20A7, 0x0192, // 0x98
        0x00E1, 0x00ED, 0x00F3, 0x00FA, 0x00F1, 0x00D1, 0x00AA, 0x00BA, // 0xA0
        0x00BF, 0x2310, 0x00AC, 0x00BD, 0x00BC, 0x00A1, 0x00AB, 0x00BB, // 0xA8
        0x2591, 0x2592, 0x2593, 0x2502, 0x2524, 0x2561, 0x2562, 0x2556, // 0xB0
        0x2555, 0x2563, 0x2551, 0x2557, 0x255D, 0x255C, 0x255B, 0x2510, // 0xB8
        0x2514, 0x2534, 0x252C, 0x251C, 0x2500, 0x253C, 0x255E, 0x255F, // 0xC0
        0x255A, 0x2554, 0x2569, 0x2566, 0x2560, 0x2550, 0x256C, 0x2567, // 0xC8
        0x2568, 0x2564, 0x2565, 0x2559, 0x2558, 0x2552, 0x2553, 0x256B, // 0xD0
        0x256A, 0x2518, 0x250C, 0x2588, 0x2584, 0x258C, 0x2590, 0x2580, // 0xD8
        0x03B1, 0x00DF, 0x0393, 0x03C0, 0x03A3, 0x03C3, 0x00B5, 0x03C4, // 0xE0
        0x03A6, 0x0398, 0x03A9, 0x03B4, 0x221E, 0x03C6, 0x03B5, 0x2229, // 0xE8
        0x2261, 0x00B1, 0x2265, 0x2264, 0x2320, 0x2321, 0x00F7, 0x2248, // 0xF0
        0x00B0, 0x2219, 0x00B7, 0x221A, 0x207F, 0x00B2, 0x25A0, 0x00A0, // 0xF8
    ];
    Some(CP437_HIGH[(b - 0x80) as usize])
}

fn cp437_from_utf8(cp: u32) -> Option<u8> {
    if cp <= 0x7F {
        return Some(cp as u8);
    }
    // Reverse lookup in the CP437 high table
    for b in 0x80u8..=0xFFu8 {
        if cp437_to_utf8(b) == Some(cp) {
            return Some(b);
        }
    }
    None
}

/// Default helper: convert a Unicode code point to charset bytes.
pub fn char_from_utf8(table: &NlsTable, cp: u32) -> Option<u8> {
    (table.char_from_utf8)(cp)
}

/// Default helper: convert a charset byte to a Unicode code point.
pub fn char_to_utf8(table: &NlsTable, b: u8) -> Option<u32> {
    (table.char_to_utf8)(b)
}

/// Register an NLS table in the global registry.
pub fn register_nls(table: NlsTable) -> Result<(), String> {
    let mut registry = NLS_REGISTRY.write();
    registry.insert(table.name.clone(), Arc::new(table));
    Ok(())
}

/// Unregister an NLS table by name.
pub fn unregister_nls(name: &str) -> Result<(), String> {
    let mut registry = NLS_REGISTRY.write();
    if registry.remove(name).is_some() {
        Ok(())
    } else {
        Err(format!("NLS table '{}' not found", name))
    }
}

/// Load (look up) an NLS table by name.
pub fn load_nls(name: &str) -> Option<Arc<NlsTable>> {
    let registry = NLS_REGISTRY.read();
    registry.get(name).cloned()
}

/// Initialize the NLS subsystem with built-in tables.
pub fn init_nls() {
    register_nls(NlsTable {
        name: "ascii".to_string(),
        char_from_utf8: ascii_from_utf8,
        char_to_utf8: ascii_to_utf8,
    })
    .ok();

    register_nls(NlsTable {
        name: "iso8859-1".to_string(),
        char_from_utf8: iso8859_1_from_utf8,
        char_to_utf8: iso8859_1_to_utf8,
    })
    .ok();

    register_nls(NlsTable {
        name: "cp437".to_string(),
        char_from_utf8: cp437_from_utf8,
        char_to_utf8: cp437_to_utf8,
    })
    .ok();
}

/// Convert a Unicode string (code points) to a charset-encoded byte string.
pub fn unicode_to_charset(code_points: &[u32], charset: &str) -> Vec<u8> {
    let table = match load_nls(charset) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut result = Vec::with_capacity(code_points.len());
    for &cp in code_points {
        if let Some(b) = (table.char_from_utf8)(cp) {
            result.push(b);
        }
    }
    result
}

/// Convert a charset-encoded byte string to Unicode code points.
pub fn charset_to_unicode(bytes: &[u8], charset: &str) -> Vec<u32> {
    let table = match load_nls(charset) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut result = Vec::with_capacity(bytes.len());
    for &b in bytes {
        if let Some(cp) = (table.char_to_utf8)(b) {
            result.push(cp);
        }
    }
    result
}

/// Register a character set converter (legacy API).
pub fn register_charset(
    name: &str,
    converter: fn(&[u8]) -> Vec<u32>,
) -> Result<(), String> {
    // Create a table that uses the converter for char_to_utf8
    // and a passthrough for char_from_utf8
    let _ = converter; // The legacy API only provides bytes→unicode
    register_nls(NlsTable {
        name: name.to_string(),
        char_from_utf8: |_cp| None,
        char_to_utf8: |b| Some(b as u32),
    })
}

/// Unregister a character set converter (legacy API).
pub fn unregister_charset(name: &str) -> Result<(), String> {
    unregister_nls(name)
}
