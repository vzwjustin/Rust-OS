//! strinfo matching `gio/strinfo.c`.
//!
//! The string info map is an efficient data structure for small sets of items,
//! used by GSettings schemas for:
//! 1. `<choices>` with a list of valid strings
//! 2. `<alias>` by mapping one string to another
//! 3. Enumerated types by mapping strings to integer values (and back)
//!
//! The map is an array of `u32` words. Each entry is an integer value followed
//! by a specially formatted string. Strings start with `0xff` (enum) or `0xfe`
//! (alias), followed by content, nul padding to 4-byte alignment (min 8 bytes),
//! and a trailing `0xff` byte.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::vec::Vec;
use core::convert::TryInto;

/// Maximum number of 32-bit words a string can occupy (65 chars + padding).
const STRINFO_MAX_WORDS: usize = 17;

/// Converts a string into the word-encoded format used by strinfo.
///
/// Returns the number of words used, or 0 if the string is too long.
fn strinfo_string_to_words(string: &str, alias: bool) -> Option<Vec<u32>> {
    let bytes = string.as_bytes();
    let size = bytes.len();

    let n_words = core::cmp::max(2usize, (size + 6) >> 2);
    if n_words > STRINFO_MAX_WORDS {
        return None;
    }

    let mut words = vec![0u32; n_words];
    let marker: u8 = if alias { 0xfe } else { 0xff };
    words[0] = marker.to_le_bytes()[0] as u32 | (marker as u32) << 8;

    // Actually, we need to set the first byte and last byte properly.
    // The C code does: words[0] = GUINT32_TO_LE(alias ? 0xfe : 0xff);
    // words[n_words-1] = GUINT32_TO_BE(0xff);
    // memcpy(((char*)words) + 1, string, size+1);

    // Let's do this byte-by-byte for correctness.
    let mut word_bytes = vec![0u8; n_words * 4];
    word_bytes[0] = if alias { 0xfe } else { 0xff };
    // Copy string + nul terminator starting at byte offset 1
    for (i, &b) in bytes.iter().enumerate() {
        word_bytes[1 + i] = b;
    }
    // byte at 1 + size is already 0 (nul)
    // Set trailing 0xff at the last byte
    word_bytes[n_words * 4 - 1] = 0xff;

    // Convert to u32 words (little-endian)
    for i in 0..n_words {
        let start = i * 4;
        words[i] = u32::from_le_bytes([
            word_bytes[start],
            word_bytes[start + 1],
            word_bytes[start + 2],
            word_bytes[start + 3],
        ]);
    }

    Some(words)
}

/// Scans the strinfo array for a sequence of words.
///
/// Returns the word index of the match, or `None`.
fn strinfo_scan(strinfo: &[u32], words: &[u32]) -> Option<usize> {
    let n_words = words.len();
    if strinfo.len() < n_words {
        return None;
    }

    let mut i = 0usize;
    while i <= strinfo.len() - n_words {
        let mut j = 0;
        while j < n_words {
            if strinfo[i + j] != words[j] {
                break;
            }
            j += 1;
        }
        if j == n_words {
            return Some(i);
        }
        i += if j > 0 { j } else { 1 };
    }
    None
}

/// Finds a string in the strinfo array (searching from word index 1 onward).
///
/// Returns the word index relative to the start of `strinfo` (including the
/// initial integer), or `None`.
pub fn strinfo_find_string(strinfo: &[u32], string: &str, alias: bool) -> Option<usize> {
    if strinfo.is_empty() {
        return None;
    }
    let words = strinfo_string_to_words(string, alias)?;
    // C code: strinfo_scan(strinfo + 1, length - 1, words, n_words)
    // which returns index relative to strinfo+1, so we add 1
    let idx = strinfo_scan(&strinfo[1..], &words)?;
    Some(idx + 1)
}

/// Finds an integer value in the strinfo array.
///
/// Returns the word index, or `None`. Ensures the value is bounded by
/// `0xff` bytes on either side (or start of array).
pub fn strinfo_find_integer(strinfo: &[u32], value: u32) -> Option<usize> {
    let le_value = u32::from_le(value);
    for (i, &word) in strinfo.iter().enumerate() {
        if word == le_value {
            let bytes = word.to_le_bytes();
            let _prev_ok = i == 0 || {
                let prev_bytes = strinfo[i - 1].to_le_bytes();
                prev_bytes[3] == 0xff
            };
            let _next_ok = bytes[0] == 0 || {
                // Check byte at offset 4 (start of next word)
                if i + 1 < strinfo.len() {
                    let next_bytes = strinfo[i + 1].to_le_bytes();
                    next_bytes[0] == 0xff
                } else {
                    false
                }
            };
            // Actually the C code checks: charinfo[-1] == 0xff && charinfo[4] == 0xff
            // where charinfo = (const guchar*)&strinfo[i]
            // So charinfo[-1] is the last byte of the previous word
            // and charinfo[4] is the first byte of the next word
            let _charinfo = word.to_le_bytes();
            let prev_byte_ok = i == 0 || { strinfo[i - 1].to_le_bytes()[3] == 0xff };
            let next_byte_ok = i + 1 < strinfo.len() && { strinfo[i + 1].to_le_bytes()[0] == 0xff };
            if prev_byte_ok && next_byte_ok {
                return Some(i);
            }
        }
    }
    None
}

/// Checks if a string is valid (not an alias) in the strinfo map.
pub fn strinfo_is_string_valid(strinfo: &[u32], string: &str) -> bool {
    strinfo_find_string(strinfo, string, false).is_some()
}

/// Gets the enum value for a given string.
///
/// Returns `Some(value)` if found, `None` otherwise.
pub fn strinfo_enum_from_string(strinfo: &[u32], string: &str) -> Option<u32> {
    let index = strinfo_find_string(strinfo, string, false)?;
    Some(u32::from_le(strinfo[index - 1]))
}

/// Gets the string for a given enum value.
///
/// Returns the string, or `None` if not found.
pub fn strinfo_string_from_enum(strinfo: &[u32], value: u32) -> Option<String> {
    let index = strinfo_find_integer(strinfo, value)?;
    // The string starts 1 byte past the start of the next word (index+1)
    let _word_bytes = strinfo[index + 1].to_le_bytes();
    // The string starts at byte offset 1 within the word
    // We need to read the string starting from byte 1 of word at index+1
    let mut result = Vec::new();
    for word_idx in (index + 1)..strinfo.len() {
        let bytes = strinfo[word_idx].to_le_bytes();
        let start = if word_idx == index + 1 { 1 } else { 0 };
        for bi in start..4 {
            if bytes[bi] == 0 {
                // Found nul terminator
                return Some(String::from_utf8_lossy(&result).into_owned());
            }
            result.push(bytes[bi]);
        }
    }
    None
}

/// Gets the target string for an alias.
///
/// Returns the target string, or `None` if the alias is not found.
pub fn strinfo_string_from_alias(strinfo: &[u32], alias: &str) -> Option<String> {
    let index = strinfo_find_string(strinfo, alias, true)?;
    // The integer at index gives the word offset of the target
    let target_index = u32::from_le(strinfo[index - 1]) as usize;
    // append_alias stores the word index of the encoded target string.
    let mut result = Vec::new();
    for word_idx in target_index..strinfo.len() {
        let bytes = strinfo[word_idx].to_le_bytes();
        let start = if word_idx == target_index { 1 } else { 0 };
        for bi in start..4 {
            if bytes[bi] == 0 {
                return Some(String::from_utf8_lossy(&result).into_owned());
            }
            if bytes[bi] != 0xff && bytes[bi] != 0xfe {
                result.push(bytes[bi]);
            }
        }
    }
    None
}

/// Enumerates all non-alias strings in the strinfo map.
pub fn strinfo_enumerate(strinfo: &[u32]) -> Vec<String> {
    let mut result = Vec::new();
    if strinfo.is_empty() {
        return result;
    }

    let total_bytes = strinfo.len() * 4;
    let mut byte_idx = 4; // Skip first word (initial integer)

    while byte_idx < total_bytes {
        let word_idx = byte_idx / 4;
        let byte_offset = byte_idx % 4;
        let bytes = strinfo[word_idx].to_le_bytes();

        if bytes[byte_offset] == 0xff {
            // Start of a string entry; read the string
            let mut string_bytes = Vec::new();
            byte_idx += 1;
            while byte_idx < total_bytes {
                let wi = byte_idx / 4;
                let bo = byte_idx % 4;
                let b = strinfo[wi].to_le_bytes()[bo];
                if b == 0 {
                    // nul terminator
                    result.push(String::from_utf8_lossy(&string_bytes).into_owned());
                    // Skip to the next 0xff
                    byte_idx += 1;
                    while byte_idx < total_bytes {
                        let wi2 = byte_idx / 4;
                        let bo2 = byte_idx % 4;
                        if strinfo[wi2].to_le_bytes()[bo2] == 0xff {
                            byte_idx += 1; // Skip the 0xff
                                           // Skip the integer word (4 bytes)
                            byte_idx = (byte_idx + 3) & !3; // align
                            byte_idx += 4; // skip integer
                            break;
                        }
                        byte_idx += 1;
                    }
                    break;
                }
                string_bytes.push(b);
                byte_idx += 1;
            }
        } else {
            byte_idx += 1;
        }
    }

    result
}

/// A builder for constructing strinfo maps.
#[derive(Default)]
pub struct StrInfoBuilder {
    data: Vec<u8>,
}

impl StrInfoBuilder {
    /// Creates a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an item (string + integer value) to the builder.
    pub fn append_item(&mut self, string: &str, value: u32) {
        let le_value = value.to_le_bytes();
        self.data.extend_from_slice(&le_value);
        let words = strinfo_string_to_words(string, false).expect("string too long");
        for w in &words {
            self.data.extend_from_slice(&w.to_le_bytes());
        }
    }

    /// Appends an alias to the builder.
    ///
    /// Returns `true` if the target was found and the alias was added.
    pub fn append_alias(&mut self, alias: &str, target: &str) -> bool {
        let strinfo: Vec<u32> = self
            .data
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let index = match strinfo_find_string(&strinfo, target, false) {
            Some(i) => i,
            None => return false,
        };
        let le_value = (index as u32).to_le_bytes();
        self.data.extend_from_slice(&le_value);
        let words = strinfo_string_to_words(alias, true).expect("alias too long");
        for w in &words {
            self.data.extend_from_slice(&w.to_le_bytes());
        }
        true
    }

    /// Checks if the builder contains a string (as either an item or alias).
    pub fn contains(&self, string: &str) -> bool {
        let strinfo: Vec<u32> = self
            .data
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        strinfo_find_string(&strinfo, string, false).is_some()
            || strinfo_find_string(&strinfo, string, true).is_some()
    }

    /// Checks if the builder contains a given value.
    pub fn contains_value(&self, value: u32) -> bool {
        let strinfo: Vec<u32> = self
            .data
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        strinfo_string_from_enum(&strinfo, value).is_some()
    }

    /// Returns the raw bytes of the builder.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Returns the strinfo as a slice of u32 words.
    pub fn as_words(&self) -> Vec<u32> {
        self.data
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_append_and_lookup() {
        let mut builder = StrInfoBuilder::new();
        builder.append_item("foo", 1);
        builder.append_item("bar", 2);

        let words = builder.as_words();
        assert!(strinfo_is_string_valid(&words, "foo"));
        assert!(strinfo_is_string_valid(&words, "bar"));
        assert!(!strinfo_is_string_valid(&words, "baz"));

        assert_eq!(strinfo_enum_from_string(&words, "foo"), Some(1));
        assert_eq!(strinfo_enum_from_string(&words, "bar"), Some(2));
        assert_eq!(strinfo_enum_from_string(&words, "baz"), None);
    }

    #[test]
    fn test_string_from_enum() {
        let mut builder = StrInfoBuilder::new();
        builder.append_item("foo", 1);
        builder.append_item("bar", 2);

        let words = builder.as_words();
        assert_eq!(strinfo_string_from_enum(&words, 1), Some("foo".to_string()));
        assert_eq!(strinfo_string_from_enum(&words, 2), Some("bar".to_string()));
        assert_eq!(strinfo_string_from_enum(&words, 3), None);
    }

    #[test]
    fn test_alias() {
        let mut builder = StrInfoBuilder::new();
        builder.append_item("foo", 1);
        builder.append_item("bar", 2);
        assert!(builder.append_alias("baz", "bar"));

        let words = builder.as_words();
        assert_eq!(
            strinfo_string_from_alias(&words, "baz"),
            Some("bar".to_string())
        );
        assert_eq!(strinfo_string_from_alias(&words, "qux"), None);
    }

    #[test]
    fn test_contains() {
        let mut builder = StrInfoBuilder::new();
        builder.append_item("foo", 1);
        builder.append_item("bar", 2);
        assert!(builder.contains("foo"));
        assert!(builder.contains("bar"));
        assert!(!builder.contains("baz"));
        assert!(builder.contains_value(1));
        assert!(builder.contains_value(2));
        assert!(!builder.contains_value(3));
    }

    #[test]
    fn test_enumerate() {
        let mut builder = StrInfoBuilder::new();
        builder.append_item("foo", 1);
        builder.append_item("bar", 2);
        builder.append_alias("baz", "bar");

        let words = builder.as_words();
        let enums = strinfo_enumerate(&words);
        assert!(enums.contains(&"foo".to_string()));
        assert!(enums.contains(&"bar".to_string()));
        // baz is an alias, should not be enumerated
        assert!(!enums.contains(&"baz".to_string()));
    }

    #[test]
    fn test_alias_target_not_found() {
        let mut builder = StrInfoBuilder::new();
        builder.append_item("foo", 1);
        assert!(!builder.append_alias("baz", "nonexistent"));
    }
}
