//! `xdgmimeint` matching `gio/xdgmime/xdgmimeint.h`.
//!
//! XDG MIME internal utilities: UTF-8 helpers, byte swapping,
//! base name extraction, and text/binary detection.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// Unicode character type (mirrors `xdg_unichar_t`).
pub type XdgUnichar = u32;

/// 8-bit unsigned char type (mirrors `xdg_uchar8_t`).
pub type XdgUchar8 = u8;

/// 16-bit unsigned int type (mirrors `xdg_uint16_t`).
pub type XdgUint16 = u16;

/// 32-bit unsigned int type (mirrors `xdg_uint32_t`).
pub type XdgUint32 = u32;

/// Swaps a big-endian 16-bit value to little-endian
/// (mirrors `SWAP_BE16_TO_LE16`).
pub fn swap_be16_to_le16(val: u16) -> u16 {
    ((val & 0xFF) << 8) | ((val >> 8) & 0xFF)
}

/// Swaps a big-endian 32-bit value to little-endian
/// (mirrors `SWAP_BE32_TO_LE32`).
pub fn swap_be32_to_le32(val: u32) -> u32 {
    ((val & 0xFF000000) >> 24)
        | ((val & 0x00FF0000) >> 8)
        | ((val & 0x0000FF00) << 8)
        | ((val & 0x000000FF) << 24)
}

/// UTF-8 skip table: number of bytes for each leading byte.
/// Mirrors `_xdg_utf8_skip`.
pub fn utf8_char_size(leading_byte: u8) -> usize {
    if leading_byte < 0x80 {
        1
    } else if leading_byte < 0xC0 {
        1
    } else if leading_byte < 0xE0 {
        2
    } else if leading_byte < 0xF0 {
        3
    } else if leading_byte < 0xF8 {
        4
    } else {
        1
    }
}

/// Converts a UTF-8 sequence to a UCS-4 code point
/// (mirrors `_xdg_utf8_to_ucs4`).
pub fn utf8_to_ucs4(source: &str) -> XdgUnichar {
    let bytes = source.as_bytes();
    if bytes.is_empty() {
        return 0;
    }
    let size = utf8_char_size(bytes[0]);
    if bytes.len() < size {
        return bytes[0] as XdgUnichar;
    }
    match size {
        1 => bytes[0] as XdgUnichar,
        2 => ((bytes[0] as XdgUnichar & 0x1F) << 6) | (bytes[1] as XdgUnichar & 0x3F),
        3 => {
            ((bytes[0] as XdgUnichar & 0x0F) << 12)
                | ((bytes[1] as XdgUnichar & 0x3F) << 6)
                | (bytes[2] as XdgUnichar & 0x3F)
        }
        4 => {
            ((bytes[0] as XdgUnichar & 0x07) << 18)
                | ((bytes[1] as XdgUnichar & 0x3F) << 12)
                | ((bytes[2] as XdgUnichar & 0x3F) << 6)
                | (bytes[3] as XdgUnichar & 0x3F)
        }
        _ => bytes[0] as XdgUnichar,
    }
}

/// Converts a UCS-4 code point to lowercase
/// (mirrors `_xdg_ucs4_to_lower`).
pub fn ucs4_to_lower(c: XdgUnichar) -> XdgUnichar {
    if (0x41..=0x5A).contains(&c) {
        c + 32
    } else if (0x0410..=0x042F).contains(&c) {
        c + 32
    } else {
        c
    }
}

/// Validates a UTF-8 string (mirrors `_xdg_utf8_validate`).
pub fn utf8_validate(source: &str) -> bool {
    core::str::from_utf8(source.as_bytes()).is_ok()
}

/// Converts a string to UCS-4 array (mirrors `_xdg_convert_to_ucs4`).
pub fn convert_to_ucs4(source: &str) -> Vec<XdgUnichar> {
    let mut result = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let size = utf8_char_size(bytes[i]);
        if i + size > bytes.len() {
            break;
        }
        let chunk = core::str::from_utf8(&bytes[i..i + size]);
        if let Ok(s) = chunk {
            result.push(utf8_to_ucs4(s));
        } else {
            result.push(bytes[i] as XdgUnichar);
        }
        i += size;
    }
    result
}

/// Reverses a UCS-4 array in place (mirrors `_xdg_reverse_ucs4`).
pub fn reverse_ucs4(source: &mut [XdgUnichar]) {
    source.reverse();
}

/// Returns the base name of a file path (mirrors `_xdg_get_base_name`).
pub fn get_base_name(file_name: &str) -> &str {
    if let Some(pos) = file_name.rfind(|c| c == '/' || c == '\\') {
        &file_name[pos + 1..]
    } else {
        file_name
    }
}

/// Determines if data is text or binary (mirrors `_xdg_binary_or_text_fallback`).
/// Returns "text/plain" if text, "application/octet-stream" if binary.
pub fn binary_or_text_fallback(data: &[u8]) -> &'static str {
    if data.is_empty() {
        return "inode/x-empty";
    }
    let mut non_text = 0;
    let len = data.len().min(1024);
    for &b in &data[..len] {
        if b == 0 {
            return "application/octet-stream";
        }
        if b < 0x09 || (b > 0x0d && b < 0x20) {
            non_text += 1;
        }
    }
    if (non_text as f64 / len as f64) < 0.3 {
        "text/plain"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_be16() {
        assert_eq!(swap_be16_to_le16(0x1234), 0x3412);
        assert_eq!(swap_be16_to_le16(0x0000), 0x0000);
    }

    #[test]
    fn test_swap_be32() {
        assert_eq!(swap_be32_to_le32(0x12345678), 0x78563412);
        assert_eq!(swap_be32_to_le32(0x00000000), 0x00000000);
    }

    #[test]
    fn test_utf8_char_size() {
        assert_eq!(utf8_char_size(b'A'), 1);
        assert_eq!(utf8_char_size(0xC2), 2);
        assert_eq!(utf8_char_size(0xE0), 3);
        assert_eq!(utf8_char_size(0xF0), 4);
    }

    #[test]
    fn test_utf8_to_ucs4_ascii() {
        assert_eq!(utf8_to_ucs4("A"), 0x41);
    }

    #[test]
    fn test_utf8_to_ucs4_multibyte() {
        assert_eq!(utf8_to_ucs4("\u{00E9}"), 0xE9);
    }

    #[test]
    fn test_ucs4_to_lower() {
        assert_eq!(ucs4_to_lower(b'A' as u32), b'a' as u32);
        assert_eq!(ucs4_to_lower(b'a' as u32), b'a' as u32);
        assert_eq!(ucs4_to_lower(b'0' as u32), b'0' as u32);
    }

    #[test]
    fn test_utf8_validate() {
        assert!(utf8_validate("hello"));
        assert!(utf8_validate("\u{00E9}"));
    }

    #[test]
    fn test_convert_to_ucs4() {
        let result = convert_to_ucs4("AB");
        assert_eq!(result, vec![0x41, 0x42]);
    }

    #[test]
    fn test_reverse_ucs4() {
        let mut data = vec![1, 2, 3, 4];
        reverse_ucs4(&mut data);
        assert_eq!(data, vec![4, 3, 2, 1]);
    }

    #[test]
    fn test_get_base_name() {
        assert_eq!(get_base_name("/path/to/file.txt"), "file.txt");
        assert_eq!(get_base_name("file.txt"), "file.txt");
        assert_eq!(get_base_name("C:\\path\\file.txt"), "file.txt");
        assert_eq!(get_base_name("/"), "");
    }

    #[test]
    fn test_binary_or_text_fallback() {
        assert_eq!(binary_or_text_fallback(b""), "inode/x-empty");
        assert_eq!(binary_or_text_fallback(b"Hello, world!"), "text/plain");
        assert_eq!(
            binary_or_text_fallback(&[0, 1, 2, 3]),
            "application/octet-stream"
        );
    }
}
