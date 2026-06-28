//! GLib Win32 I/O compatibility (`giowin32.c`).

use alloc::string::String;
use alloc::vec::Vec;
use core::char::decode_utf16;

#[must_use]
pub fn utf8_to_utf16_nul(s: &str) -> Vec<u16> {
    let mut out: Vec<u16> = s.encode_utf16().collect();
    out.push(0);
    out
}

#[must_use]
pub fn utf16_to_utf8_lossy(input: &[u16]) -> String {
    let len = input.iter().position(|&c| c == 0).unwrap_or(input.len());
    decode_utf16(input[..len].iter().copied())
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

#[must_use]
pub fn normalize_path(path: &str) -> String {
    path.chars()
        .map(|c| if c == '\\' { '/' } else { c })
        .collect()
}

#[must_use]
pub fn has_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_and_paths() {
        let wide = utf8_to_utf16_nul("C:\\Temp");
        assert_eq!(utf16_to_utf8_lossy(&wide), "C:\\Temp");
        assert_eq!(normalize_path("C:\\Temp\\x"), "C:/Temp/x");
        assert!(has_drive_prefix("C:/Temp"));
    }
}
