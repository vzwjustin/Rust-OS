//! Unicode normalization (NFD) for Latin-1 and common composed characters.
//!
//! Provides a compact embedded decomposition table and string-level NFD
//! normalization. Full Unicode coverage is intentionally deferred.

use crate::unicode::{combining_class, NormalizeMode};
use alloc::string::String;
use alloc::vec::Vec;
use core::char;

/// Alias matching the migration API name.
pub type UnicodeNormalizeMode = NormalizeMode;

/// Canonical decomposition entry: `composed` → (`base`, `combining_mark`).
#[derive(Clone, Copy, Debug)]
struct DecompEntry {
    composed: u32,
    base: u32,
    mark: u32,
}

/// Embedded decomposition table (Latin-1 supplement + common Latin Extended-A).
///
/// Entries must remain sorted by `composed` for binary search.
const DECOMP_TABLE: &[DecompEntry] = &[
    // Uppercase Latin-1
    DecompEntry {
        composed: 0x00C0,
        base: 0x0041,
        mark: 0x0300,
    }, // À
    DecompEntry {
        composed: 0x00C1,
        base: 0x0041,
        mark: 0x0301,
    }, // Á
    DecompEntry {
        composed: 0x00C2,
        base: 0x0041,
        mark: 0x0302,
    }, // Â
    DecompEntry {
        composed: 0x00C3,
        base: 0x0041,
        mark: 0x0303,
    }, // Ã
    DecompEntry {
        composed: 0x00C4,
        base: 0x0041,
        mark: 0x0308,
    }, // Ä
    DecompEntry {
        composed: 0x00C5,
        base: 0x0041,
        mark: 0x030A,
    }, // Å
    DecompEntry {
        composed: 0x00C6,
        base: 0x0041,
        mark: 0x0045,
    }, // Æ
    DecompEntry {
        composed: 0x00C7,
        base: 0x0043,
        mark: 0x0327,
    }, // Ç
    DecompEntry {
        composed: 0x00C8,
        base: 0x0045,
        mark: 0x0300,
    }, // È
    DecompEntry {
        composed: 0x00C9,
        base: 0x0045,
        mark: 0x0301,
    }, // É
    DecompEntry {
        composed: 0x00CA,
        base: 0x0045,
        mark: 0x0302,
    }, // Ê
    DecompEntry {
        composed: 0x00CB,
        base: 0x0045,
        mark: 0x0308,
    }, // Ë
    DecompEntry {
        composed: 0x00CC,
        base: 0x0049,
        mark: 0x0300,
    }, // Ì
    DecompEntry {
        composed: 0x00CD,
        base: 0x0049,
        mark: 0x0301,
    }, // Í
    DecompEntry {
        composed: 0x00CE,
        base: 0x0049,
        mark: 0x0302,
    }, // Î
    DecompEntry {
        composed: 0x00CF,
        base: 0x0049,
        mark: 0x0308,
    }, // Ï
    DecompEntry {
        composed: 0x00D1,
        base: 0x004E,
        mark: 0x0303,
    }, // Ñ
    DecompEntry {
        composed: 0x00D2,
        base: 0x004F,
        mark: 0x0300,
    }, // Ò
    DecompEntry {
        composed: 0x00D3,
        base: 0x004F,
        mark: 0x0301,
    }, // Ó
    DecompEntry {
        composed: 0x00D4,
        base: 0x004F,
        mark: 0x0302,
    }, // Ô
    DecompEntry {
        composed: 0x00D5,
        base: 0x004F,
        mark: 0x0303,
    }, // Õ
    DecompEntry {
        composed: 0x00D6,
        base: 0x004F,
        mark: 0x0308,
    }, // Ö
    DecompEntry {
        composed: 0x00D8,
        base: 0x004F,
        mark: 0x0338,
    }, // Ø
    DecompEntry {
        composed: 0x00D9,
        base: 0x0055,
        mark: 0x0300,
    }, // Ù
    DecompEntry {
        composed: 0x00DA,
        base: 0x0055,
        mark: 0x0301,
    }, // Ú
    DecompEntry {
        composed: 0x00DB,
        base: 0x0055,
        mark: 0x0302,
    }, // Û
    DecompEntry {
        composed: 0x00DC,
        base: 0x0055,
        mark: 0x0308,
    }, // Ü
    DecompEntry {
        composed: 0x00DD,
        base: 0x0059,
        mark: 0x0301,
    }, // Ý
    // Lowercase Latin-1
    DecompEntry {
        composed: 0x00E0,
        base: 0x0061,
        mark: 0x0300,
    }, // à
    DecompEntry {
        composed: 0x00E1,
        base: 0x0061,
        mark: 0x0301,
    }, // á
    DecompEntry {
        composed: 0x00E2,
        base: 0x0061,
        mark: 0x0302,
    }, // â
    DecompEntry {
        composed: 0x00E3,
        base: 0x0061,
        mark: 0x0303,
    }, // ã
    DecompEntry {
        composed: 0x00E4,
        base: 0x0061,
        mark: 0x0308,
    }, // ä
    DecompEntry {
        composed: 0x00E5,
        base: 0x0061,
        mark: 0x030A,
    }, // å
    DecompEntry {
        composed: 0x00E6,
        base: 0x0061,
        mark: 0x0065,
    }, // æ
    DecompEntry {
        composed: 0x00E7,
        base: 0x0063,
        mark: 0x0327,
    }, // ç
    DecompEntry {
        composed: 0x00E8,
        base: 0x0065,
        mark: 0x0300,
    }, // è
    DecompEntry {
        composed: 0x00E9,
        base: 0x0065,
        mark: 0x0301,
    }, // é
    DecompEntry {
        composed: 0x00EA,
        base: 0x0065,
        mark: 0x0302,
    }, // ê
    DecompEntry {
        composed: 0x00EB,
        base: 0x0065,
        mark: 0x0308,
    }, // ë
    DecompEntry {
        composed: 0x00EC,
        base: 0x0069,
        mark: 0x0300,
    }, // ì
    DecompEntry {
        composed: 0x00ED,
        base: 0x0069,
        mark: 0x0301,
    }, // í
    DecompEntry {
        composed: 0x00EE,
        base: 0x0069,
        mark: 0x0302,
    }, // î
    DecompEntry {
        composed: 0x00EF,
        base: 0x0069,
        mark: 0x0308,
    }, // ï
    DecompEntry {
        composed: 0x00F1,
        base: 0x006E,
        mark: 0x0303,
    }, // ñ
    DecompEntry {
        composed: 0x00F2,
        base: 0x006F,
        mark: 0x0300,
    }, // ò
    DecompEntry {
        composed: 0x00F3,
        base: 0x006F,
        mark: 0x0301,
    }, // ó
    DecompEntry {
        composed: 0x00F4,
        base: 0x006F,
        mark: 0x0302,
    }, // ô
    DecompEntry {
        composed: 0x00F5,
        base: 0x006F,
        mark: 0x0303,
    }, // õ
    DecompEntry {
        composed: 0x00F6,
        base: 0x006F,
        mark: 0x0308,
    }, // ö
    DecompEntry {
        composed: 0x00F8,
        base: 0x006F,
        mark: 0x0338,
    }, // ø
    DecompEntry {
        composed: 0x00F9,
        base: 0x0075,
        mark: 0x0300,
    }, // ù
    DecompEntry {
        composed: 0x00FA,
        base: 0x0075,
        mark: 0x0301,
    }, // ú
    DecompEntry {
        composed: 0x00FB,
        base: 0x0075,
        mark: 0x0302,
    }, // û
    DecompEntry {
        composed: 0x00FC,
        base: 0x0075,
        mark: 0x0308,
    }, // ü
    DecompEntry {
        composed: 0x00FD,
        base: 0x0079,
        mark: 0x0301,
    }, // ý
    DecompEntry {
        composed: 0x00FF,
        base: 0x0079,
        mark: 0x0308,
    }, // ÿ
    // Latin Extended-A (common)
    DecompEntry {
        composed: 0x0100,
        base: 0x0041,
        mark: 0x0304,
    }, // Ā
    DecompEntry {
        composed: 0x0101,
        base: 0x0061,
        mark: 0x0304,
    }, // ā
    DecompEntry {
        composed: 0x0102,
        base: 0x0041,
        mark: 0x0306,
    }, // Ă
    DecompEntry {
        composed: 0x0103,
        base: 0x0061,
        mark: 0x0306,
    }, // ă
    DecompEntry {
        composed: 0x0104,
        base: 0x0041,
        mark: 0x0328,
    }, // Ą
    DecompEntry {
        composed: 0x0105,
        base: 0x0061,
        mark: 0x0328,
    }, // ą
    DecompEntry {
        composed: 0x0106,
        base: 0x0043,
        mark: 0x0301,
    }, // Ć
    DecompEntry {
        composed: 0x0107,
        base: 0x0063,
        mark: 0x0301,
    }, // ć
    DecompEntry {
        composed: 0x0108,
        base: 0x0043,
        mark: 0x0302,
    }, // Ĉ
    DecompEntry {
        composed: 0x0109,
        base: 0x0063,
        mark: 0x0302,
    }, // ĉ
    DecompEntry {
        composed: 0x010A,
        base: 0x0043,
        mark: 0x0307,
    }, // Ċ
    DecompEntry {
        composed: 0x010B,
        base: 0x0063,
        mark: 0x0307,
    }, // ċ
    DecompEntry {
        composed: 0x010C,
        base: 0x0043,
        mark: 0x030C,
    }, // Č
    DecompEntry {
        composed: 0x010D,
        base: 0x0063,
        mark: 0x030C,
    }, // č
    DecompEntry {
        composed: 0x0128,
        base: 0x0049,
        mark: 0x0303,
    }, // Ĩ
    DecompEntry {
        composed: 0x0129,
        base: 0x0069,
        mark: 0x0303,
    }, // ĩ
    DecompEntry {
        composed: 0x0152,
        base: 0x004F,
        mark: 0x0045,
    }, // Œ
    DecompEntry {
        composed: 0x0153,
        base: 0x006F,
        mark: 0x0065,
    }, // œ
    DecompEntry {
        composed: 0x0160,
        base: 0x0053,
        mark: 0x030C,
    }, // Š
    DecompEntry {
        composed: 0x0161,
        base: 0x0073,
        mark: 0x030C,
    }, // š
    DecompEntry {
        composed: 0x0178,
        base: 0x0059,
        mark: 0x0308,
    }, // Ÿ
    DecompEntry {
        composed: 0x017D,
        base: 0x005A,
        mark: 0x030C,
    }, // Ž
    DecompEntry {
        composed: 0x017E,
        base: 0x007A,
        mark: 0x030C,
    }, // ž
];

/// Compatibility-only decompositions (NFKD / `NormalizeMode::All`).
const COMPAT_DECOMP_TABLE: &[DecompEntry] = &[DecompEntry {
    composed: 0x00DF,
    base: 0x0073,
    mark: 0x0073,
}]; // ß → ss

fn lookup_decomp(ch: u32, compat: bool) -> Option<(u32, u32)> {
    if compat {
        if let Some(entry) = COMPAT_DECOMP_TABLE.iter().find(|e| e.composed == ch) {
            if entry.mark == 0 {
                return None;
            }
            return Some((entry.base, entry.mark));
        }
    }

    let mut lo = 0usize;
    let mut hi = DECOMP_TABLE.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let entry = &DECOMP_TABLE[mid];
        match ch.cmp(&entry.composed) {
            core::cmp::Ordering::Equal => return Some((entry.base, entry.mark)),
            core::cmp::Ordering::Less => hi = mid,
            core::cmp::Ordering::Greater => lo = mid + 1,
        }
    }
    None
}

/// Returns the canonical two-codepoint decomposition of `ch`, if present.
pub fn unichar_canonical_decomposition(ch: u32) -> Option<(u32, u32)> {
    lookup_decomp(ch, false)
}

/// Decompose a single code point into NFD (or NFKD when `compat` is true).
fn decompose_char(ch: u32, compat: bool) -> Vec<u32> {
    if let Some((base, mark)) = lookup_decomp(ch, compat) {
        let mut out = vec![base];
        if mark >= 0x0300 {
            out.push(mark);
        } else if mark != 0 {
            out.push(mark);
        }
        return out;
    }
    vec![ch]
}

/// Compose `base` + `mark` into a precomposed character when known.
fn try_compose(base: u32, mark: u32) -> Option<u32> {
    DECOMP_TABLE
        .iter()
        .find(|e| e.base == base && e.mark == mark)
        .map(|e| e.composed)
}

/// Apply canonical ordering to a codepoint buffer (Unicode §3.11).
fn canonical_ordering(buf: &mut [u32]) {
    if buf.len() < 2 {
        return;
    }
    let mut swapped = true;
    while swapped {
        swapped = false;
        let mut last_class = combining_class(buf[0]);
        for i in 0..buf.len() - 1 {
            let next_class = combining_class(buf[i + 1]);
            if next_class != 0 && last_class > next_class {
                buf.swap(i, i + 1);
                swapped = true;
                if i > 0 {
                    last_class = combining_class(buf[i - 1]);
                } else {
                    last_class = 0;
                }
            } else {
                last_class = next_class;
            }
        }
    }
}

/// Compose adjacent starter + combining mark pairs (NFC / NFKC step 2).
fn compose_adjacent(buf: &mut Vec<u32>) {
    let mut i = 0;
    while i + 1 < buf.len() {
        let starter_class = combining_class(buf[i]);
        let mark_class = combining_class(buf[i + 1]);
        if starter_class == 0 && mark_class != 0 {
            if let Some(composed) = try_compose(buf[i], buf[i + 1]) {
                buf[i] = composed;
                buf.remove(i + 1);
                continue;
            }
        }
        i += 1;
    }
}

/// Normalize a single code point according to `mode`.
pub fn unichar_normalize(ch: u32, mode: UnicodeNormalizeMode) -> Vec<u32> {
    let compat = matches!(mode, NormalizeMode::All | NormalizeMode::AllCompose);
    let compose = matches!(
        mode,
        NormalizeMode::DefaultCompose | NormalizeMode::AllCompose
    );

    let mut out = decompose_char(ch, compat);
    if compose {
        compose_adjacent(&mut out);
    }
    out
}

/// Normalize a UTF-8 string to NFD (canonical decomposition + ordering).
pub fn normalize_nfd(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }

    let mut codepoints = Vec::new();
    for ch in s.chars() {
        let c = ch as u32;
        codepoints.extend(decompose_char(c, false));
    }
    canonical_ordering(&mut codepoints);

    let mut out = String::new();
    for cp in codepoints {
        if let Some(c) = char::from_u32(cp) {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unicode::NormalizeMode;

    #[test]
    fn decomp_e_acute() {
        let (base, mark) = unichar_canonical_decomposition('é' as u32).unwrap();
        assert_eq!(base, 'e' as u32);
        assert_eq!(mark, 0x0301);
    }

    #[test]
    fn decomp_a_diaeresis() {
        let (base, mark) = unichar_canonical_decomposition('Ä' as u32).unwrap();
        assert_eq!(base, 'A' as u32);
        assert_eq!(mark, 0x0308);
    }

    #[test]
    fn no_decomp_ascii() {
        assert!(unichar_canonical_decomposition('A' as u32).is_none());
        assert!(unichar_canonical_decomposition('z' as u32).is_none());
        assert!(unichar_canonical_decomposition('7' as u32).is_none());
    }

    #[test]
    fn normalize_nfd_e_acute() {
        let nfd = normalize_nfd("é");
        assert_eq!(nfd, "e\u{0301}");
    }

    #[test]
    fn normalize_nfd_a_diaeresis() {
        let nfd = normalize_nfd("Ä");
        assert_eq!(nfd, "A\u{0308}");
    }

    #[test]
    fn normalize_nfd_ascii_unchanged() {
        assert_eq!(normalize_nfd("hello"), "hello");
        assert_eq!(normalize_nfd("ABC123"), "ABC123");
    }

    #[test]
    fn normalize_nfd_empty() {
        assert_eq!(normalize_nfd(""), "");
    }

    #[test]
    fn normalize_nfd_mixed_string() {
        let nfd = normalize_nfd("café");
        assert_eq!(nfd, "cafe\u{0301}");
    }

    #[test]
    fn unichar_normalize_nfd() {
        let out = unichar_normalize('é' as u32, NormalizeMode::Default);
        assert_eq!(out, vec!['e' as u32, 0x0301]);
    }

    #[test]
    fn unichar_normalize_nfc() {
        let out = unichar_normalize('é' as u32, NormalizeMode::DefaultCompose);
        assert_eq!(out, vec!['é' as u32]);
    }

    #[test]
    fn unichar_normalize_ascii() {
        let out = unichar_normalize('X' as u32, NormalizeMode::Default);
        assert_eq!(out, vec!['X' as u32]);
    }

    #[test]
    fn decomp_table_has_minimum_entries() {
        assert!(DECOMP_TABLE.len() >= 50);
    }
}
