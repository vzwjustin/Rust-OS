//! String utility functions from `gstrfuncs.h` (non-printf subset).

use crate::prelude::*;
use crate::checked::{checked_add_size, checked_mul_size};
use crate::Size;

/// Returns the length of `s` in bytes (`strlen`).
#[inline]
pub fn strlen(s: &str) -> Size {
    s.len()
}

/// Lexicographic compare (`strcmp`).
#[inline]
pub fn strcmp(s1: &str, s2: &str) -> i32 {
    match s1.cmp(s2) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

/// ASCII case-insensitive compare (`g_ascii_strcasecmp`).
pub fn ascii_strcasecmp(s1: &str, s2: &str) -> i32 {
    let mut i1 = s1.bytes();
    let mut i2 = s2.bytes();

    loop {
        match (i1.next(), i2.next()) {
            (Some(c1), Some(c2)) => {
                let c1 = c1.to_ascii_lowercase();
                let c2 = c2.to_ascii_lowercase();
                if c1 != c2 {
                    return i32::from(c1) - i32::from(c2);
                }
            }
            (Some(c1), None) => return i32::from(c1),
            (None, Some(c2)) => return -i32::from(c2),
            (None, None) => return 0,
        }
    }
}

/// ASCII case-insensitive compare (`g_strcasecmp` without locale).
#[inline]
pub fn strcasecmp(s1: &str, s2: &str) -> i32 {
    ascii_strcasecmp(s1, s2)
}

/// Whether `str` begins with `prefix` (`g_str_has_prefix`).
pub fn str_has_prefix(str: &str, prefix: &str) -> bool {
    str.as_bytes().starts_with(prefix.as_bytes())
}

/// Whether `str` ends with `suffix` (`g_str_has_suffix`).
pub fn str_has_suffix(str: &str, suffix: &str) -> bool {
    str.as_bytes().ends_with(suffix.as_bytes())
}

/// Duplicate a string (`g_strdup`).
pub fn strdup(str: Option<&str>) -> Option<String> {
    str.map(str::to_owned)
}

/// Duplicate up to `n` bytes (`g_strndup`).
///
/// Returns a buffer of `n + 1` bytes, always nul-terminated at index `n`.
/// The first `n` bytes follow `strncpy` semantics (padded with nuls when `str`
/// is shorter than `n`).
pub fn strndup(str: Option<&str>, n: Size) -> Option<Vec<u8>> {
    let s = str?;
    if n == Size::MAX {
        return None;
    }
    let mut buf = vec![0u8; n + 1];
    let copy_len = n.min(s.len());
    buf[..copy_len].copy_from_slice(&s.as_bytes()[..copy_len]);
    Some(buf)
}

/// Concatenate strings (`g_strconcat`). Returns `None` when `parts` is empty.
pub fn strconcat(parts: &[&str]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    let mut total = 0usize;
    for part in parts {
        total = checked_add_size(total, part.len()).expect("strconcat overflow");
    }
    let mut out = String::with_capacity(total);
    for part in parts {
        out.push_str(part);
    }
    Some(out)
}

/// Join strings with an optional separator (`g_strjoin` / `g_strjoinv`).
pub fn strjoin(separator: Option<&str>, parts: &[&str]) -> String {
    strjoinv(separator, parts)
}

/// Join a slice of strings (`g_strjoinv`).
pub fn strjoinv(separator: Option<&str>, parts: &[&str]) -> String {
    let separator = separator.unwrap_or("");
    match parts.len() {
        0 => String::new(),
        1 => parts[0].to_owned(),
        _ => {
            let sep_len = separator.len();
            let separators =
                checked_mul_size(sep_len, parts.len() - 1).expect("join separator overflow");
            let mut total = separators;
            for part in parts {
                total = checked_add_size(total, part.len()).expect("join overflow");
            }
            let mut out = String::with_capacity(total);
            out.push_str(parts[0]);
            for part in &parts[1..] {
                out.push_str(separator);
                out.push_str(part);
            }
            out
        }
    }
}

/// Whether `c` is ASCII whitespace per GLib's `g_ascii_isspace`.
fn is_ascii_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// Remove leading whitespace in place (`g_strchug`).
pub fn strchug(s: &mut String) {
    let start = s.bytes().position(|c| !is_ascii_space(c)).unwrap_or(s.len());
    if start > 0 {
        s.drain(..start);
    }
}

/// Remove trailing whitespace in place (`g_strchomp`).
pub fn strchomp(s: &mut String) {
    let len = s.len();
    let mut end = len;
    while end > 0 && is_ascii_space(s.as_bytes()[end - 1]) {
        end -= 1;
    }
    if end < len {
        s.truncate(end);
    }
}

/// Remove leading and trailing whitespace (`g_strstrip`).
pub fn strstrip(s: &mut String) {
    strchug(s);
    strchomp(s);
}

// ---------------------------------------------------------------------------
// ASCII character classification (g_ascii_is*)
// ---------------------------------------------------------------------------

/// Returns `true` if `c` is an ASCII alphanumeric character.
pub fn ascii_isalnum(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

/// Returns `true` if `c` is an ASCII alphabetic character.
pub fn ascii_isalpha(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

/// Returns `true` if `c` is an ASCII control character.
pub fn ascii_iscntrl(c: u8) -> bool {
    c.is_ascii_control()
}

/// Returns `true` if `c` is an ASCII digit.
pub fn ascii_isdigit(c: u8) -> bool {
    c.is_ascii_digit()
}

/// Returns `true` if `c` is an ASCII graphic character (printable, non-space).
pub fn ascii_isgraph(c: u8) -> bool {
    c.is_ascii_graphic()
}

/// Returns `true` if `c` is an ASCII lowercase letter.
pub fn ascii_islower(c: u8) -> bool {
    c.is_ascii_lowercase()
}

/// Returns `true` if `c` is an ASCII printable character (space or graphic).
pub fn ascii_isprint(c: u8) -> bool {
    (0x20u8..0x7f).contains(&c)
}

/// Returns `true` if `c` is an ASCII punctuation character.
pub fn ascii_ispunct(c: u8) -> bool {
    c.is_ascii_punctuation()
}

/// Returns `true` if `c` is an ASCII whitespace character.
pub fn ascii_isspace(c: u8) -> bool {
    is_ascii_space(c)
}

/// Returns `true` if `c` is an ASCII uppercase letter.
pub fn ascii_isupper(c: u8) -> bool {
    c.is_ascii_uppercase()
}

/// Returns `true` if `c` is an ASCII hexadecimal digit.
pub fn ascii_isxdigit(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

/// Convert `c` to ASCII lowercase (`g_ascii_tolower`).
pub fn ascii_tolower(c: u8) -> u8 {
    c.to_ascii_lowercase()
}

/// Convert `c` to ASCII uppercase (`g_ascii_toupper`).
pub fn ascii_toupper(c: u8) -> u8 {
    c.to_ascii_uppercase()
}

/// Returns the numeric value of ASCII digit `c`, or -1 if not a digit.
pub fn ascii_digit_value(c: u8) -> i32 {
    if c.is_ascii_digit() {
        i32::from(c - b'0')
    } else {
        -1
    }
}

/// Returns the numeric value of ASCII hex digit `c`, or -1 if not hex.
pub fn ascii_xdigit_value(c: u8) -> i32 {
    match c {
        b'0'..=b'9' => i32::from(c - b'0'),
        b'a'..=b'f' => i32::from(c - b'a' + 10),
        b'A'..=b'F' => i32::from(c - b'A' + 10),
        _ => -1,
    }
}

// ---------------------------------------------------------------------------
// String search functions
// ---------------------------------------------------------------------------

/// Search for `needle` in `haystack` (`g_strstr_len`).
///
/// Returns the byte offset of the first occurrence, or `None`.
pub fn strstr_len(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.find(needle)
}

/// Search for `needle` in `haystack` from the right (`g_strrstr`).
///
/// Returns the byte offset of the last occurrence, or `None`.
pub fn strrstr(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    haystack.rfind(needle)
}

/// Reverse a string in place (`g_strreverse`).
pub fn strreverse(s: &mut str) {
    let bytes = unsafe { s.as_bytes_mut() };
    bytes.reverse();
}

/// Replace delimiter characters in `s` with `new_delimiter` (`g_strdelimit`).
pub fn strdelimit(s: &mut str, delimiters: &str, new_delimiter: char) {
    let delim_bytes = delimiters.as_bytes();
    let new_byte = new_delimiter as u8;
    let bytes = unsafe { s.as_bytes_mut() };
    for b in bytes.iter_mut() {
        if delim_bytes.contains(b) {
            *b = new_byte;
        }
    }
}

/// Canonicalize `s`: replace any char not in `valid_chars` with `substitutor` (`g_strcanon`).
pub fn strcanon(s: &mut str, valid_chars: &str, substitutor: char) {
    let valid_bytes = valid_chars.as_bytes();
    let sub_byte = substitutor as u8;
    let bytes = unsafe { s.as_bytes_mut() };
    for b in bytes.iter_mut() {
        if !valid_bytes.contains(b) {
            *b = sub_byte;
        }
    }
}

/// ASCII string-to-integer conversion (`g_ascii_strtoull`).
///
/// Parses an unsigned 64-bit integer from `nptr` in the given `base`.
/// Returns `(value, remaining_str)`.
pub fn ascii_strtoull(nptr: &str, base: u32) -> (u64, &str) {
    let bytes = nptr.as_bytes();
    let mut i = 0;

    // Skip leading whitespace
    while i < bytes.len() && is_ascii_space(bytes[i]) {
        i += 1;
    }

    // Optional sign
    let mut neg = false;
    if i < bytes.len() && bytes[i] == b'+' {
        i += 1;
    } else if i < bytes.len() && bytes[i] == b'-' {
        neg = true;
        i += 1;
    }

    // Optional base prefix
    let mut radix = base;
    if (radix == 0 || radix == 16) && i + 1 < bytes.len() && bytes[i] == b'0' && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
        i += 2;
        radix = 16;
    } else if radix == 0 {
        if i < bytes.len() && bytes[i] == b'0' {
            radix = 8;
        } else {
            radix = 10;
        }
    }

    let start = i;
    let mut result: u64 = 0;
    while i < bytes.len() {
        let d = ascii_xdigit_value(bytes[i]);
        if d < 0 || d as u32 >= radix {
            break;
        }
        result = result
            .wrapping_mul(radix as u64)
            .wrapping_add(d as u64);
        i += 1;
    }

    if i == start {
        return (0, nptr);
    }

    if neg {
        result = result.wrapping_neg();
    }

    (result, &nptr[i..])
}

/// ASCII string-to-signed-integer conversion (`g_ascii_strtoll`).
///
/// Parses a signed 64-bit integer from `nptr` in the given `base`.
/// Returns `(value, remaining_str)`.
pub fn ascii_strtoll(nptr: &str, base: u32) -> (i64, &str) {
    // ascii_strtoull already applies wrapping_neg for negative inputs,
    // so the bit-pattern reinterpretation is all that's needed here.
    let (val, rest) = ascii_strtoull(nptr, base);
    (val as i64, rest)
}

/// Duplicate a string with up to `n` bytes, returning a `String` (`g_strndup`-like, but UTF-8 safe).
pub fn strndup_str(s: &str, n: usize) -> String {
    let end = n.min(s.len());
    s[..end].to_owned()
}

/// Split a string by `delimiter` (`g_strsplit` simplified).
///
/// Returns a `Vec` of substrings. A maximum of `max_tokens` splits is performed;
/// `0` means unlimited.
pub fn strsplit(s: &str, delimiter: &str, max_tokens: u32) -> Vec<String> {
    if delimiter.is_empty() {
        return vec![s.to_owned()];
    }
    if max_tokens == 0 || max_tokens == u32::MAX {
        return s.split(delimiter).map(|p| p.to_owned()).collect();
    }
    let parts: Vec<&str> = s.splitn(max_tokens as usize + 1, delimiter).collect();
    parts.into_iter().map(|p| p.to_owned()).collect()
}

/// Split a string by any character in `delimiters` (`g_strsplit_set`).
///
/// Returns a `Vec` of substrings. A maximum of `max_tokens` splits is performed;
/// `0` means unlimited.
pub fn strsplit_set(s: &str, delimiters: &str, max_tokens: u32) -> Vec<String> {
    if delimiters.is_empty() {
        return vec![s.to_owned()];
    }
    if max_tokens == 0 {
        return s.split(|c| delimiters.contains(c)).map(|p| p.to_owned()).collect();
    }
    let parts: Vec<&str> = s.splitn(max_tokens as usize + 1, |c| delimiters.contains(c)).collect();
    parts.into_iter().map(|p| p.to_owned()).collect()
}

/// ASCII case-insensitive compare of first `n` bytes (`g_ascii_strncasecmp`).
pub fn ascii_strncasecmp(s1: &str, s2: &str, n: usize) -> i32 {
    let b1 = s1.as_bytes();
    let b2 = s2.as_bytes();
    let len = n.min(b1.len()).min(b2.len());
    for i in 0..len {
        let c1 = b1[i].to_ascii_lowercase();
        let c2 = b2[i].to_ascii_lowercase();
        if c1 != c2 {
            return i32::from(c1) - i32::from(c2);
        }
    }
    if n <= b1.len().min(b2.len()) {
        0
    } else if b1.len() < b2.len() {
        -i32::from(b2[n - 1].to_ascii_lowercase())
    } else if b1.len() > b2.len() {
        i32::from(b1[n - 1].to_ascii_lowercase())
    } else {
        0
    }
}

/// Convert string to ASCII lowercase (`g_ascii_strdown`).
pub fn ascii_strdown(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Convert string to ASCII uppercase (`g_ascii_strup`).
pub fn ascii_strup(s: &str) -> String {
    s.to_ascii_uppercase()
}

/// Returns `true` if `str` is pure ASCII (`g_str_is_ascii`).
pub fn str_is_ascii(s: &str) -> bool {
    s.is_ascii()
}

/// Create a string of `length` filled with `fill_char` (`g_strnfill`).
pub fn strnfill(length: usize, fill_char: u8) -> Vec<u8> {
    vec![fill_char; length]
}

/// Returns `true` if `strv` contains `s` (`g_strv_contains`).
pub fn strv_contains(strv: &[&str], s: &str) -> bool {
    strv.contains(&s)
}

/// Returns `true` if two string arrays are equal (`g_strv_equal`).
pub fn strv_equal(strv1: &[&str], strv2: &[&str]) -> bool {
    strv1.len() == strv2.len() && strv1.iter().zip(strv2.iter()).all(|(a, b)| a == b)
}

/// Returns the length of a string slice (`g_strv_length`).
pub fn strv_length(strv: &[&str]) -> usize {
    strv.len()
}

/// Escape special characters in a string (`g_strescape`).
///
/// Escapes `\n`, `\t`, `\r`, `\\`, and non-ASCII bytes as `\xHH`.
/// Characters in `exceptions` are not escaped.
pub fn strescape(source: &str, exceptions: Option<&str>) -> String {
    let except_bytes = exceptions.map(|s| s.as_bytes()).unwrap_or(&[]);
    let mut result = String::with_capacity(source.len() * 2);
    for b in source.bytes() {
        match b {
            b'\n' => result.push_str("\\n"),
            b'\t' => result.push_str("\\t"),
            b'\r' => result.push_str("\\r"),
            b'\\' => result.push_str("\\\\"),
            _ if !(0x20..0x7f).contains(&b) => {
                if except_bytes.contains(&b) {
                    result.push(b as char);
                } else {
                    result.push_str(&format!("\\{:03o}", b));
                }
            }
            _ => result.push(b as char),
        }
    }
    result
}

/// Unescape a string produced by `strescape` (`g_strcompress`).
pub fn strcompress(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut result = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'n' => { result.push('\n'); i += 2; }
                b't' => { result.push('\t'); i += 2; }
                b'r' => { result.push('\r'); i += 2; }
                b'\\' => { result.push('\\'); i += 2; }
                b'0'..=b'7' => {
                    // Octal escape: up to 3 octal digits
                    let mut val: u32 = 0;
                    let mut j = i + 1;
                    while j < bytes.len() && j < i + 4 && (bytes[j] >= b'0' && bytes[j] <= b'7') {
                        val = val * 8 + (bytes[j] - b'0') as u32;
                        j += 1;
                    }
                    result.push(val as u8 as char);
                    i = j;
                }
                _ => { result.push('\\'); i += 1; }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

/// Duplicate a `Vec<&str>` into a `Vec<String>` (`g_strdupv`).
pub fn strdupv(strv: &[&str]) -> Vec<String> {
    strv.iter().map(|s| (*s).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLIB_TEST_STRING: &str = "el dorado ";

    #[test]
    fn strdup_null_and_empty() {
        assert_eq!(strdup(None), None);

        let s = strdup(Some(GLIB_TEST_STRING)).unwrap();
        assert_eq!(s, GLIB_TEST_STRING);

        let s = strdup(Some("")).unwrap();
        assert_eq!(s, "");
    }

    #[test]
    fn strndup_cases() {
        assert_eq!(strndup(None, 3), None);
        let padded = strndup(Some("aaaa"), 5).unwrap();
        assert_eq!(padded.len(), 6);
        assert_eq!(&padded[0..5], b"aaaa\0");
        assert_eq!(padded[5], 0);
        let short = strndup(Some("aaaa"), 2).unwrap();
        assert_eq!(&short[0..2], b"aa");
        assert_eq!(short[2], 0);
        assert_eq!(strndup(Some("aaaa"), Size::MAX), None);
    }

    #[test]
    fn strconcat_cases() {
        assert_eq!(
            strconcat(&[GLIB_TEST_STRING]).as_deref(),
            Some(GLIB_TEST_STRING)
        );
        assert_eq!(
            strconcat(&[GLIB_TEST_STRING, GLIB_TEST_STRING, GLIB_TEST_STRING]).as_deref(),
            Some("el dorado el dorado el dorado ")
        );
        assert_eq!(strconcat(&[]), None);
    }

    #[test]
    fn strjoinv_cases() {
        let strings = ["string1", "string2"];
        assert_eq!(strjoinv(Some(":"), &strings), "string1:string2");
        assert_eq!(strjoinv(None, &strings), "string1string2");
        assert_eq!(strjoinv(None, &[] as &[&str]), "");
    }

    #[test]
    fn strjoin_cases() {
        assert_eq!(strjoin(None, &[] as &[&str]), "");
        assert_eq!(strjoin(Some(":"), &[] as &[&str]), "");
        assert_eq!(strjoin(None, &[GLIB_TEST_STRING]), GLIB_TEST_STRING);
        assert_eq!(
            strjoin(
                None,
                &[GLIB_TEST_STRING, GLIB_TEST_STRING, GLIB_TEST_STRING]
            ),
            "el dorado el dorado el dorado "
        );
        assert_eq!(
            strjoin(
                Some(":"),
                &[GLIB_TEST_STRING, GLIB_TEST_STRING, GLIB_TEST_STRING]
            ),
            "el dorado :el dorado :el dorado "
        );
    }

    #[test]
    fn ascii_strcasecmp_cases() {
        assert_eq!(ascii_strcasecmp("FroboZZ", "frobozz"), 0);
        assert_eq!(ascii_strcasecmp("frobozz", "frobozz"), 0);
        assert_ne!(ascii_strcasecmp("FROBOZZ", "froboz"), 0);
        assert_ne!(ascii_strcasecmp("FROBOZZ", "froboz"), 0);
        assert_eq!(ascii_strcasecmp("", ""), 0);
        assert_eq!(ascii_strcasecmp("!#%&/()", "!#%&/()"), 0);
        assert!(ascii_strcasecmp("a", "b") < 0);
        assert!(ascii_strcasecmp("b", "a") > 0);
    }

    #[test]
    fn strcmp_strlen() {
        assert_eq!(strcmp("abc", "abc"), 0);
        assert!(strcmp("abc", "abd") < 0);
        assert_eq!(strlen("hello"), 5);
    }

    #[test]
    fn str_has_prefix_suffix() {
        assert!(!str_has_prefix("aa", "aaa"));
        assert!(!str_has_prefix("foo", "bar"));
        assert!(!str_has_prefix("foo", "foobar"));
        assert!(str_has_prefix("foobar", "foo"));
        assert!(str_has_prefix("foo", ""));
        assert!(str_has_prefix("", ""));

        assert!(!str_has_suffix("aa", "aaa"));
        assert!(!str_has_suffix("foo", "bar"));
        assert!(!str_has_suffix("bar", "foobar"));
        assert!(str_has_suffix("foobar", "bar"));
        assert!(str_has_suffix("foo", ""));
        assert!(str_has_suffix("", ""));
    }

    fn check_strchug(input: &str, expected: &str) {
        let mut tmp = input.to_owned();
        strchug(&mut tmp);
        assert_eq!(tmp, expected);
    }

    #[test]
    fn strchug_cases() {
        check_strchug("", "");
        check_strchug(" ", "");
        check_strchug("\t\r\n ", "");
        check_strchug(" a", "a");
        check_strchug("  a", "a");
        check_strchug("a a", "a a");
        check_strchug(" a a", "a a");
    }

    fn check_strchomp(input: &str, expected: &str) {
        let mut tmp = input.to_owned();
        strchomp(&mut tmp);
        assert_eq!(tmp, expected);
    }

    #[test]
    fn strchomp_cases() {
        check_strchomp("", "");
        check_strchomp(" ", "");
        check_strchomp(" \t\r\n", "");
        check_strchomp("a ", "a");
        check_strchomp("a  ", "a");
        check_strchomp("a a", "a a");
        check_strchomp("a a ", "a a");
    }

    #[test]
    fn strstrip_cases() {
        let mut s = "  hello  ".to_owned();
        strstrip(&mut s);
        assert_eq!(s, "hello");
    }

    #[test]
    fn ascii_char_classification() {
        assert!(ascii_isalpha(b'A'));
        assert!(ascii_isalpha(b'z'));
        assert!(!ascii_isalpha(b'5'));

        assert!(ascii_isdigit(b'0'));
        assert!(!ascii_isdigit(b'a'));

        assert!(ascii_isalnum(b'A'));
        assert!(ascii_isalnum(b'5'));
        assert!(!ascii_isalnum(b'!'));

        assert!(ascii_isspace(b' '));
        assert!(ascii_isspace(b'\t'));
        assert!(!ascii_isspace(b'x'));

        assert!(ascii_isupper(b'A'));
        assert!(!ascii_isupper(b'a'));

        assert!(ascii_islower(b'a'));
        assert!(!ascii_islower(b'A'));

        assert!(ascii_isxdigit(b'A'));
        assert!(ascii_isxdigit(b'f'));
        assert!(ascii_isxdigit(b'0'));
        assert!(!ascii_isxdigit(b'g'));
    }

    #[test]
    fn ascii_tolower_toupper() {
        assert_eq!(ascii_tolower(b'A'), b'a');
        assert_eq!(ascii_tolower(b'Z'), b'z');
        assert_eq!(ascii_tolower(b'a'), b'a');
        assert_eq!(ascii_tolower(b'5'), b'5');

        assert_eq!(ascii_toupper(b'a'), b'A');
        assert_eq!(ascii_toupper(b'z'), b'Z');
        assert_eq!(ascii_toupper(b'A'), b'A');
        assert_eq!(ascii_toupper(b'5'), b'5');
    }

    #[test]
    fn test_ascii_digit_value() {
        assert_eq!(ascii_digit_value(b'0'), 0);
        assert_eq!(ascii_digit_value(b'9'), 9);
        assert_eq!(ascii_digit_value(b'a'), -1);
    }

    #[test]
    fn test_ascii_xdigit_value() {
        assert_eq!(ascii_xdigit_value(b'0'), 0);
        assert_eq!(ascii_xdigit_value(b'9'), 9);
        assert_eq!(ascii_xdigit_value(b'A'), 10);
        assert_eq!(ascii_xdigit_value(b'f'), 15);
        assert_eq!(ascii_xdigit_value(b'g'), -1);
    }

    #[test]
    fn strstr_and_strrstr() {
        assert_eq!(strstr_len("hello world", "world"), Some(6));
        assert_eq!(strstr_len("hello world", "xyz"), None);
        assert_eq!(strstr_len("hello", ""), Some(0));

        assert_eq!(strrstr("hello hello", "hello"), Some(6));
        assert_eq!(strrstr("hello", "xyz"), None);
    }

    #[test]
    fn strreverse_test() {
        let mut s = "hello".to_owned();
        strreverse(&mut s);
        assert_eq!(s, "olleh");
    }

    #[test]
    fn strsplit_test() {
        assert_eq!(
            strsplit("a:b:c", ":", 0),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            strsplit("a:b:c", ":", 1),
            vec!["a", "b:c"]
        );
        assert_eq!(
            strsplit("a:b:c", ":", 2),
            vec!["a", "b", "c"]
        );
        assert_eq!(strsplit("abc", ":", 0), vec!["abc"]);
    }

    #[test]
    fn ascii_strtoull_test() {
        assert_eq!(ascii_strtoull("42", 10).0, 42);
        assert_eq!(ascii_strtoull("  -42", 10).0, (-42i64 as u64));
        assert_eq!(ascii_strtoull("0xff", 16).0, 255);
        assert_eq!(ascii_strtoull("0xFF", 0).0, 255);
        assert_eq!(ascii_strtoull("123abc", 10), (123, "abc"));
    }

    #[test]
    fn strdelimit_test() {
        let mut s = "a_b-c.d".to_owned();
        strdelimit(&mut s, "_-.", '_');
        assert_eq!(s, "a_b_c_d");
    }

    #[test]
    fn strcanon_test() {
        let mut s = "hello world!".to_owned();
        strcanon(&mut s, "abcdefghijklmnopqrstuvwxyz ", '?');
        assert_eq!(s, "hello world?");
    }

    #[test]
    fn ascii_strncasecmp_test() {
        assert_eq!(ascii_strncasecmp("Hello", "hello", 5), 0);
        assert_eq!(ascii_strncasecmp("Hello", "HELLO", 3), 0);
        assert_eq!(ascii_strncasecmp("abc", "abd", 3), -1);
    }

    #[test]
    fn ascii_strdown_up_test() {
        assert_eq!(ascii_strdown("Hello World"), "hello world");
        assert_eq!(ascii_strup("Hello World"), "HELLO WORLD");
    }

    #[test]
    fn str_is_ascii_test() {
        assert!(str_is_ascii("Hello World"));
        assert!(!str_is_ascii("café"));
    }

    #[test]
    fn strnfill_test() {
        assert_eq!(strnfill(5, b'-'), b"-----");
        assert_eq!(strnfill(0, b'x'), b"");
    }

    #[test]
    fn strsplit_set_test() {
        assert_eq!(
            strsplit_set("a,b;c.d", ",;.", 0),
            vec!["a", "b", "c", "d"]
        );
        assert_eq!(
            strsplit_set("a,b;c", ",;", 1),
            vec!["a", "b;c"]
        );
    }

    #[test]
    fn strv_test() {
        let v = ["a", "b", "c"];
        assert_eq!(strv_length(&v), 3);
        assert!(strv_contains(&v, "b"));
        assert!(!strv_contains(&v, "d"));
        assert!(strv_equal(&v, &["a", "b", "c"]));
        assert!(!strv_equal(&v, &["a", "b"]));
    }

    #[test]
    fn strescape_compress_test() {
        let original = "hello\nworld\t!";
        let escaped = strescape(original, None);
        assert!(escaped.contains("\\n"));
        assert!(escaped.contains("\\t"));
        let compressed = strcompress(&escaped);
        assert_eq!(compressed, original);
    }

    #[test]
    fn strdupv_test() {
        let v = strdupv(&["a", "b", "c"]);
        assert_eq!(v, vec!["a", "b", "c"]);
    }
}
