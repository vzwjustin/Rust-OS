//! giowin32_private matching `gio/giowin32-private.c`.
//!
//! Private Windows-specific GIO utility functions: UTF-16 string helpers,
//! wide-character basename finding, and case folding for Windows path
//! comparisons.
//!
//! In this no_std port, we implement the UTF-16 helpers using Rust's
//! built-in UTF-16 support.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::vec::Vec;

/// Computes the length of a UTF-16 string (number of code units until nul).
///
/// Mirrors `g_utf16_len`.
pub fn utf16_len(s: &[u16]) -> usize {
    s.iter().position(|&c| c == 0).unwrap_or(s.len())
}

/// Duplicates a UTF-16 string.
///
/// Mirrors `g_wcsdup`.
pub fn wcsdup(s: &[u16]) -> Vec<u16> {
    let len = utf16_len(s);
    s[..len].to_vec()
}

/// Finds a wide character in a UTF-16 string.
///
/// Mirrors `g_utf16_wchr`.
pub fn utf16_wchr(s: &[u16], wchr: u16) -> Option<usize> {
    s.iter().position(|&c| c == wchr)
}

/// Converts UTF-16 to UTF-8 and casefolds the result.
///
/// Mirrors `g_utf16_to_utf8_and_fold`.
pub fn utf16_to_utf8_and_fold(s: &[u16]) -> (String, String) {
    let utf8 = utf16_to_utf8(s);
    let folded = casefold(&utf8);
    (utf8, folded)
}

/// Converts UTF-16 to UTF-8.
pub fn utf16_to_utf8(s: &[u16]) -> String {
    let len = utf16_len(s);
    String::from_utf16_lossy(&s[..len])
}

/// Converts UTF-8 to UTF-16.
pub fn utf8_to_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Casefolds a UTF-8 string (simplified: ASCII lowercase).
pub fn casefold(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Finds the basename in a UTF-16 filename.
///
/// Returns the index of the first character after the last directory separator.
/// If the string ends with a separator, returns the index of the nul terminator.
/// If the string contains no separators, returns 0.
///
/// Mirrors `g_utf16_find_basename`.
pub fn utf16_find_basename(filename: &[u16]) -> usize {
    let len = utf16_len(filename);
    if len == 0 {
        return 0;
    }

    let mut i = len - 1;
    while i > 0 {
        let c = filename[i];
        if c == b'/' as u16 || c == b'\\' as u16 {
            return i + 1;
        }
        i -= 1;
    }

    // Check first character
    if filename[0] == b'/' as u16 || filename[0] == b'\\' as u16 {
        return 1;
    }

    0
}

/// Compares two UTF-16 strings case-insensitively.
///
/// Mirrors `_wcsicmp` behavior used throughout the Windows GIO code.
pub fn wcsicmp(a: &[u16], b: &[u16]) -> i32 {
    let len_a = utf16_len(a);
    let len_b = utf16_len(b);
    let min_len = core::cmp::min(len_a, len_b);

    for i in 0..min_len {
        let ca = (a[i] as u8).to_ascii_lowercase();
        let cb = (b[i] as u8).to_ascii_lowercase();
        if ca != cb {
            return ca as i32 - cb as i32;
        }
    }

    len_a as i32 - len_b as i32
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf16_len() {
        let s: Vec<u16> = vec![b'H' as u16, b'i' as u16, 0];
        assert_eq!(utf16_len(&s), 2);
        assert_eq!(utf16_len(&[0]), 0);
        assert_eq!(utf16_len(&[b'A' as u16, b'B' as u16, b'C' as u16]), 3);
    }

    #[test]
    fn test_wcsdup() {
        let s: Vec<u16> = vec![b'H' as u16, b'i' as u16, 0, b'X' as u16];
        let d = wcsdup(&s);
        assert_eq!(d, vec![b'H' as u16, b'i' as u16]);
    }

    #[test]
    fn test_utf16_wchr() {
        let s: Vec<u16> = vec![b'a' as u16, b'b' as u16, b'c' as u16, 0];
        assert_eq!(utf16_wchr(&s, b'b' as u16), Some(1));
        assert_eq!(utf16_wchr(&s, b'z' as u16), None);
    }

    #[test]
    fn test_utf16_to_utf8() {
        let s: Vec<u16> = vec![b'H' as u16, b'i' as u16, 0];
        assert_eq!(utf16_to_utf8(&s), "Hi");
    }

    #[test]
    fn test_utf8_to_utf16() {
        let s = utf8_to_utf16("Hello");
        assert_eq!(
            s,
            vec![
                b'H' as u16,
                b'e' as u16,
                b'l' as u16,
                b'l' as u16,
                b'o' as u16
            ]
        );
    }

    #[test]
    fn test_utf16_to_utf8_and_fold() {
        let s: Vec<u16> = vec![
            b'H' as u16,
            b'E' as u16,
            b'L' as u16,
            b'L' as u16,
            b'O' as u16,
            0,
        ];
        let (utf8, folded) = utf16_to_utf8_and_fold(&s);
        assert_eq!(utf8, "HELLO");
        assert_eq!(folded, "hello");
    }

    #[test]
    fn test_utf16_find_basename() {
        let path: Vec<u16> = utf8_to_utf16("C:\\dir\\file.txt");
        let base = utf16_find_basename(&path);
        assert_eq!(utf16_to_utf8(&path[base..]), "file.txt");

        let path2: Vec<u16> = utf8_to_utf16("file.txt");
        let base2 = utf16_find_basename(&path2);
        assert_eq!(base2, 0);

        let path3: Vec<u16> = utf8_to_utf16("/usr/bin/");
        let base3 = utf16_find_basename(&path3);
        // Should point to after the last separator
        assert_eq!(base3, path3.len());
    }

    #[test]
    fn test_wcsicmp() {
        let a: Vec<u16> = utf8_to_utf16("Hello");
        let b: Vec<u16> = utf8_to_utf16("hello");
        assert_eq!(wcsicmp(&a, &b), 0);

        let c: Vec<u16> = utf8_to_utf16("World");
        assert!(wcsicmp(&a, &c) != 0);
    }
}
