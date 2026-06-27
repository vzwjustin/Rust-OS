//! UTF-8 string utilities matching parts of `gunicode.h` / `guniprop.c` / `gutf8.c`.
//!
//! Provides UTF-8 validation, character iteration, and basic Unicode
//! character classification using `core` primitives. Full Unicode data
//! tables are not included; classification uses ASCII ranges and
//! basic Unicode category checks available in `core`.

use crate::prelude::*;

/// A Unicode code point (`gunichar`).
pub type Unichar = u32;

/// A UTF-16 code unit (`gunichar2`).
pub type Unichar2 = u16;

/// Returns `true` if `c` is a valid Unicode code point.
pub fn unichar_validate(c: Unichar) -> bool {
    // Valid Unicode: 0..=0x10FFFF, excluding surrogates
    c <= 0x10FFFF && !(0xD800..=0xDFFF).contains(&c)
}

/// Returns the number of bytes the UTF-8 encoding of `c` will occupy.
///
/// Returns 0 for invalid code points.
pub fn unichar_to_utf8_len(c: Unichar) -> usize {
    if c < 0x80 {
        1
    } else if c < 0x800 {
        2
    } else if c < 0x10000 {
        3
    } else if c < 0x110000 {
        4
    } else {
        0
    }
}

/// Encode a single code point as UTF-8 into `buf`.
///
/// Returns the number of bytes written, or 0 if `c` is invalid or `buf` is too small.
pub fn unichar_to_utf8(c: Unichar, buf: &mut [u8]) -> usize {
    let len = unichar_to_utf8_len(c);
    if len == 0 || buf.len() < len {
        return 0;
    }
    match len {
        1 => {
            buf[0] = c as u8;
        }
        2 => {
            buf[0] = 0xC0 | (c >> 6) as u8;
            buf[1] = 0x80 | (c & 0x3F) as u8;
        }
        3 => {
            buf[0] = 0xE0 | (c >> 12) as u8;
            buf[1] = 0x80 | ((c >> 6) & 0x3F) as u8;
            buf[2] = 0x80 | (c & 0x3F) as u8;
        }
        4 => {
            buf[0] = 0xF0 | (c >> 18) as u8;
            buf[1] = 0x80 | ((c >> 12) & 0x3F) as u8;
            buf[2] = 0x80 | ((c >> 6) & 0x3F) as u8;
            buf[3] = 0x80 | (c & 0x3F) as u8;
        }
        _ => return 0,
    }
    len
}

/// Encode a code point as a UTF-8 `String`.
pub fn unichar_to_utf8_string(c: Unichar) -> Option<String> {
    let mut buf = [0u8; 4];
    let len = unichar_to_utf8(c, &mut buf);
    if len == 0 {
        return None;
    }
    core::str::from_utf8(&buf[..len]).ok().map(|s| s.to_owned())
}

/// Decode the first UTF-8 character from `s`.
///
/// Returns `(code_point, byte_length)` or `None` if invalid.
pub fn utf8_get_char(s: &[u8]) -> Option<(Unichar, usize)> {
    if s.is_empty() {
        return None;
    }
    let b0 = s[0];
    if b0 < 0x80 {
        return Some((b0 as Unichar, 1));
    }
    let (len, mask) = if b0 & 0xE0 == 0xC0 {
        (2, 0x1F)
    } else if b0 & 0xF0 == 0xE0 {
        (3, 0x0F)
    } else if b0 & 0xF8 == 0xF0 {
        (4, 0x07)
    } else {
        return None;
    };
    if s.len() < len {
        return None;
    }
    let mut c = (b0 & mask) as Unichar;
    for &byte in &s[1..len] {
        if byte & 0xC0 != 0x80 {
            return None;
        }
        c = (c << 6) | (byte & 0x3F) as Unichar;
    }
    // Check for overlong encodings and surrogates
    if c < match len {
        2 => 0x80,
        3 => 0x800,
        4 => 0x10000,
        _ => return None,
    } {
        return None;
    }
    if (0xD800..=0xDFFF).contains(&c) || c > 0x10FFFF {
        return None;
    }
    Some((c, len))
}

/// Returns the byte length of the UTF-8 character starting at `s[0]`.
pub fn utf8_len(s: &[u8]) -> Option<usize> {
    utf8_get_char(s).map(|(_, len)| len)
}

/// Returns the number of Unicode characters in a UTF-8 string (`g_utf8_strlen`).
pub fn utf8_strlen(s: &str) -> usize {
    s.chars().count()
}

/// Validate that `s` is valid UTF-8.
pub fn utf8_validate(s: &[u8]) -> bool {
    let mut i = 0;
    while i < s.len() {
        match utf8_get_char(&s[i..]) {
            Some((_, len)) => i += len,
            None => return false,
        }
    }
    true
}

/// Advance `s` by `n` characters, returning the byte offset.
pub fn utf8_offset_to_pointer(s: &str, n: usize) -> usize {
    s.char_indices()
        .nth(n)
        .map(|(offset, _)| offset)
        .unwrap_or(s.len())
}

/// Count the number of characters from byte offset `start` to `end`.
pub fn utf8_pointer_to_offset(s: &str, start: usize, end: usize) -> usize {
    s[start..end.min(s.len())].chars().count()
}

/// Returns the byte offset of the previous UTF-8 character before `pos`.
pub fn utf8_prev_char(s: &str, pos: usize) -> Option<usize> {
    if pos == 0 || pos > s.len() {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = pos - 1;
    // Skip continuation bytes
    while i > 0 && bytes[i] & 0xC0 == 0x80 {
        i -= 1;
    }
    Some(i)
}

/// Returns the byte offset of the next UTF-8 character after `pos`.
pub fn utf8_next_char(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let bytes = s.as_bytes();
    let b0 = bytes[pos];
    let len = if b0 < 0x80 {
        1
    } else if b0 & 0xE0 == 0xC0 {
        2
    } else if b0 & 0xF0 == 0xE0 {
        3
    } else if b0 & 0xF8 == 0xF0 {
        4
    } else {
        // Invalid, skip 1 byte
        1
    };
    (pos + len).min(s.len())
}

// ---------------------------------------------------------------------------
// Unicode character classification (basic, no data tables)
// ---------------------------------------------------------------------------

/// Returns `true` if `c` is an ASCII-compatible alphanumeric.
pub fn unichar_isalnum(c: Unichar) -> bool {
    if c < 0x80 {
        (c as u8).is_ascii_alphanumeric()
    } else {
        // Basic check for common Unicode letters/digits
        is_unicode_letter(c) || is_unicode_digit(c)
    }
}

/// Returns `true` if `c` is an alphabetic character.
pub fn unichar_isalpha(c: Unichar) -> bool {
    if c < 0x80 {
        (c as u8).is_ascii_alphabetic()
    } else {
        is_unicode_letter(c)
    }
}

/// Returns `true` if `c` is a control character.
pub fn unichar_iscntrl(c: Unichar) -> bool {
    c < 0x20 || c == 0x7F || (0x80..=0x9F).contains(&c)
}

/// Returns `true` if `c` is a decimal digit (Nd category, basic).
pub fn unichar_isdigit(c: Unichar) -> bool {
    if c < 0x80 {
        (c as u8).is_ascii_digit()
    } else {
        // Common Unicode digit ranges
        (0x0660..=0x0669).contains(&c) // Arabic-Indic
            || (0x06F0..=0x06F9).contains(&c) // Extended Arabic-Indic
            || (0x0966..=0x096F).contains(&c) // Devanagari
            || (0xFF10..=0xFF19).contains(&c) // Fullwidth
    }
}

/// Returns `true` if `c` is a lowercase letter.
pub fn unichar_islower(c: Unichar) -> bool {
    if c < 0x80 {
        (c as u8).is_ascii_lowercase()
    } else {
        // Basic check: Unicode lowercase letters
        (0xDF..=0xF6).contains(&c) // Latin-1
            || (0xF8..=0xFF).contains(&c) // Latin-1
            || (0x3B1..=0x3C9).contains(&c) // Greek lowercase
            || (0x430..=0x44F).contains(&c) // Cyrillic lowercase
    }
}

/// Returns `true` if `c` is an uppercase letter.
pub fn unichar_isupper(c: Unichar) -> bool {
    if c < 0x80 {
        (c as u8).is_ascii_uppercase()
    } else {
        (0xC0..=0xD6).contains(&c) // Latin-1
            || (0xD8..=0xDE).contains(&c) // Latin-1
            || (0x391..=0x3A9).contains(&c) // Greek uppercase
            || (0x410..=0x42F).contains(&c) // Cyrillic uppercase
    }
}

/// Returns `true` if `c` is a whitespace character.
pub fn unichar_isspace(c: Unichar) -> bool {
    matches!(
        c,
        0x20 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D |
        0x85 | // Next line (NEL)
        0xA0 | // No-break space
        0x1680 | // Ogham space mark
        0x2000
            ..=0x200A | // Various spaces
        0x2028 | // Line separator
        0x2029 | // Paragraph separator
        0x202F | // Narrow no-break space
        0x205F | // Medium mathematical space
        0x3000 // Ideographic space
    )
}

/// Returns `true` if `c` is a printable character (not control).
pub fn unichar_isprint(c: Unichar) -> bool {
    !unichar_iscntrl(c)
}

/// Returns `true` if `c` is a punctuation character (basic).
pub fn unichar_ispunct(c: Unichar) -> bool {
    if c < 0x80 {
        (c as u8).is_ascii_punctuation()
    } else {
        // Common Unicode punctuation ranges
        (0x2010..=0x2027).contains(&c) || (0x2030..=0x205E).contains(&c)
    }
}

/// Returns `true` if `c` is a hex digit.
pub fn unichar_isxdigit(c: Unichar) -> bool {
    if c < 0x80 {
        (c as u8).is_ascii_hexdigit()
    } else {
        (0xFF10..=0xFF19).contains(&c) // Fullwidth 0-9
            || (0xFF21..=0xFF26).contains(&c) // Fullwidth A-F
            || (0xFF41..=0xFF46).contains(&c) // Fullwidth a-f
    }
}

/// Convert `c` to uppercase (basic, ASCII + Latin-1).
pub fn unichar_toupper(c: Unichar) -> Unichar {
    if c < 0x80 {
        (c as u8).to_ascii_uppercase() as Unichar
    } else if (0xE0..=0xFE).contains(&c) && c != 0xF7 {
        c - 0x20
    } else {
        c
    }
}

/// Convert `c` to lowercase (basic, ASCII + Latin-1).
pub fn unichar_tolower(c: Unichar) -> Unichar {
    if c < 0x80 {
        (c as u8).to_ascii_lowercase() as Unichar
    } else if (0xC0..=0xDE).contains(&c) && c != 0xD7 {
        c + 0x20
    } else {
        c
    }
}

/// Returns the numeric value of digit `c`, or -1.
pub fn unichar_digit_value(c: Unichar) -> i32 {
    if (0x30..=0x39).contains(&c) {
        (c - 0x30) as i32
    } else if (0x660..=0x669).contains(&c) {
        (c - 0x660) as i32
    } else if (0x6F0..=0x6F9).contains(&c) {
        (c - 0x6F0) as i32
    } else if (0x966..=0x96F).contains(&c) {
        (c - 0x966) as i32
    } else if (0xFF10..=0xFF19).contains(&c) {
        (c - 0xFF10) as i32
    } else {
        -1
    }
}

/// Returns the numeric value of hex digit `c`, or -1.
pub fn unichar_xdigit_value(c: Unichar) -> i32 {
    match c {
        0x30..=0x39 => (c - 0x30) as i32,
        0x41..=0x46 => (c - 0x41 + 10) as i32,
        0x61..=0x66 => (c - 0x61 + 10) as i32,
        0xFF10..=0xFF19 => (c - 0xFF10) as i32,
        0xFF21..=0xFF26 => (c - 0xFF21 + 10) as i32,
        0xFF41..=0xFF46 => (c - 0xFF41 + 10) as i32,
        _ => -1,
    }
}

/// Basic check for Unicode letter categories (L*).
fn is_unicode_letter(c: Unichar) -> bool {
    // Latin-1 Supplement letters
    (0xC0..=0xFF).contains(&c) && c != 0xD7 && c != 0xF7
    // Latin Extended-A
    || (0x100..=0x17F).contains(&c)
    // Latin Extended-B
    || (0x180..=0x24F).contains(&c)
    // Greek and Coptic letters
    || (0x370..=0x3FF).contains(&c)
    // Cyrillic
    || (0x400..=0x4FF).contains(&c)
    // CJK Unified Ideographs
    || (0x4E00..=0x9FFF).contains(&c)
    // Hiragana
    || (0x3040..=0x309F).contains(&c)
    // Katakana
    || (0x30A0..=0x30FF).contains(&c)
    // Hangul Syllables
    || (0xAC00..=0xD7AF).contains(&c)
}

/// Basic check for Unicode digit categories (Nd).
fn is_unicode_digit(c: Unichar) -> bool {
    (0x660..=0x669).contains(&c)
        || (0x6F0..=0x6F9).contains(&c)
        || (0x966..=0x96F).contains(&c)
        || (0xFF10..=0xFF19).contains(&c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_code_points() {
        assert!(unichar_validate(0x41));
        assert!(unichar_validate(0x10FFFF));
        assert!(!unichar_validate(0x110000));
        assert!(!unichar_validate(0xD800));
        assert!(!unichar_validate(0xDFFF));
    }

    #[test]
    fn encode_decode_ascii() {
        let mut buf = [0u8; 4];
        let len = unichar_to_utf8(0x41, &mut buf);
        assert_eq!(len, 1);
        assert_eq!(buf[0], b'A');
        let (c, l) = utf8_get_char(&buf[..len]).unwrap();
        assert_eq!(c, 0x41);
        assert_eq!(l, 1);
    }

    #[test]
    fn encode_decode_2byte() {
        let mut buf = [0u8; 4];
        let len = unichar_to_utf8(0xE9, &mut buf); // é
        assert_eq!(len, 2);
        let (c, l) = utf8_get_char(&buf[..len]).unwrap();
        assert_eq!(c, 0xE9);
        assert_eq!(l, 2);
    }

    #[test]
    fn encode_decode_3byte() {
        let mut buf = [0u8; 4];
        let len = unichar_to_utf8(0x2603, &mut buf); // ☃ snowman
        assert_eq!(len, 3);
        let (c, l) = utf8_get_char(&buf[..len]).unwrap();
        assert_eq!(c, 0x2603);
        assert_eq!(l, 3);
    }

    #[test]
    fn encode_decode_4byte() {
        let mut buf = [0u8; 4];
        let len = unichar_to_utf8(0x1F600, &mut buf); // 😀
        assert_eq!(len, 4);
        let (c, l) = utf8_get_char(&buf[..len]).unwrap();
        assert_eq!(c, 0x1F600);
        assert_eq!(l, 4);
    }

    #[test]
    fn utf8_strlen_test() {
        assert_eq!(utf8_strlen("hello"), 5);
        assert_eq!(utf8_strlen("héllo"), 5);
        assert_eq!(utf8_strlen("😀👋"), 2);
        assert_eq!(utf8_strlen(""), 0);
    }

    #[test]
    fn utf8_validate_test() {
        assert!(utf8_validate(b"hello world"));
        assert!(utf8_validate("héllo 😀".as_bytes()));
        assert!(!utf8_validate(&[0xFF, 0xFE]));
        assert!(!utf8_validate(&[0xC0, 0x80])); // overlong
    }

    #[test]
    fn utf8_next_prev() {
        let s = "aéb";
        let pos1 = utf8_next_char(s, 0);
        assert_eq!(pos1, 1);
        let pos2 = utf8_next_char(s, 1);
        assert_eq!(pos2, 3); // é is 2 bytes
        let prev = utf8_prev_char(s, 3).unwrap();
        assert_eq!(prev, 1);
    }

    #[test]
    fn offset_to_pointer() {
        let s = "aébc";
        assert_eq!(utf8_offset_to_pointer(s, 0), 0);
        assert_eq!(utf8_offset_to_pointer(s, 1), 1);
        assert_eq!(utf8_offset_to_pointer(s, 2), 3); // skip 2-byte é
        assert_eq!(utf8_offset_to_pointer(s, 4), 5);
    }

    #[test]
    fn char_classification() {
        assert!(unichar_isalpha(0x41));
        assert!(unichar_isalpha(0xE9)); // é
        assert!(!unichar_isalpha(0x31));

        assert!(unichar_isdigit(0x30));
        assert!(unichar_isdigit(0x669)); // Arabic-Indic 9
        assert!(!unichar_isdigit(0x41));

        assert!(unichar_isupper(0x41));
        assert!(unichar_islower(0x61));
        assert!(!unichar_isupper(0x61));

        assert!(unichar_isspace(0x20));
        assert!(unichar_isspace(0xA0));
        assert!(!unichar_isspace(0x41));
    }

    #[test]
    fn case_conversion() {
        assert_eq!(unichar_toupper(0x61), 0x41);
        assert_eq!(unichar_tolower(0x41), 0x61);
        assert_eq!(unichar_toupper(0xE9), 0xC9); // é -> É
        assert_eq!(unichar_tolower(0xC9), 0xE9);
    }

    #[test]
    fn digit_values() {
        assert_eq!(unichar_digit_value(0x30), 0);
        assert_eq!(unichar_digit_value(0x39), 9);
        assert_eq!(unichar_digit_value(0x669), 9);
        assert_eq!(unichar_digit_value(0x41), -1);

        assert_eq!(unichar_xdigit_value(0x41), 10);
        assert_eq!(unichar_xdigit_value(0x66), 15);
        assert_eq!(unichar_xdigit_value(0x67), -1);
    }

    #[test]
    fn unichar_to_string() {
        let s = unichar_to_utf8_string(0x2603).unwrap(); // ☃
        assert_eq!(s, "☃");
    }
}
