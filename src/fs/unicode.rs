//! Unicode normalization and encoding utilities
//!
//! This is not a mountable filesystem but a helper subsystem for Unicode
//! processing used by filesystems.  It provides NFC/NFD/NFKC/NFKD
//! normalization and case-folding for case-insensitive comparisons,
//! using a built-in Latin diacritics composition/decomposition table.

use alloc::vec::Vec;

/// Normalization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizeMode {
    /// Canonical composition (NFC).
    NFC,
    /// Canonical decomposition (NFD).
    NFD,
    /// Compatibility composition (NFKC).
    NFKC,
    /// Compatibility decomposition (NFKD).
    NFKD,
}

/// Case-folding mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CasefoldMode {
    /// Simple ASCII-only casefold.
    Simple,
    /// Full Unicode casefold (includes Latin).
    Full,
}

/// A canonical decomposition: maps a precomposed code point to its
/// base + combining mark sequence.
struct Decomposition {
    composed: u32,
    decomposed: &'static [u32],
}

/// Latin-1 Supplement + Latin Extended-A diacritics decomposition table.
/// Maps precomposed characters to (base, combining mark) pairs.
const DECOMP_TABLE: &[Decomposition] = &[
    // Latin-1 Supplement (À–ÿ)
    Decomposition { composed: 0x00C0, decomposed: &[0x0041, 0x0300] },
    Decomposition { composed: 0x00C1, decomposed: &[0x0041, 0x0301] },
    Decomposition { composed: 0x00C2, decomposed: &[0x0041, 0x0302] },
    Decomposition { composed: 0x00C3, decomposed: &[0x0041, 0x0303] },
    Decomposition { composed: 0x00C4, decomposed: &[0x0041, 0x0308] },
    Decomposition { composed: 0x00C5, decomposed: &[0x0041, 0x030A] },
    Decomposition { composed: 0x00C7, decomposed: &[0x0043, 0x0327] },
    Decomposition { composed: 0x00C8, decomposed: &[0x0045, 0x0300] },
    Decomposition { composed: 0x00C9, decomposed: &[0x0045, 0x0301] },
    Decomposition { composed: 0x00CA, decomposed: &[0x0045, 0x0302] },
    Decomposition { composed: 0x00CB, decomposed: &[0x0045, 0x0308] },
    Decomposition { composed: 0x00CC, decomposed: &[0x0049, 0x0300] },
    Decomposition { composed: 0x00CD, decomposed: &[0x0049, 0x0301] },
    Decomposition { composed: 0x00CE, decomposed: &[0x0049, 0x0302] },
    Decomposition { composed: 0x00CF, decomposed: &[0x0049, 0x0308] },
    Decomposition { composed: 0x00D1, decomposed: &[0x004E, 0x0303] },
    Decomposition { composed: 0x00D2, decomposed: &[0x004F, 0x0300] },
    Decomposition { composed: 0x00D3, decomposed: &[0x004F, 0x0301] },
    Decomposition { composed: 0x00D4, decomposed: &[0x004F, 0x0302] },
    Decomposition { composed: 0x00D5, decomposed: &[0x004F, 0x0303] },
    Decomposition { composed: 0x00D6, decomposed: &[0x004F, 0x0308] },
    Decomposition { composed: 0x00D9, decomposed: &[0x0055, 0x0300] },
    Decomposition { composed: 0x00DA, decomposed: &[0x0055, 0x0301] },
    Decomposition { composed: 0x00DB, decomposed: &[0x0055, 0x0302] },
    Decomposition { composed: 0x00DC, decomposed: &[0x0055, 0x0308] },
    Decomposition { composed: 0x00DD, decomposed: &[0x0059, 0x0301] },
    Decomposition { composed: 0x00E0, decomposed: &[0x0061, 0x0300] },
    Decomposition { composed: 0x00E1, decomposed: &[0x0061, 0x0301] },
    Decomposition { composed: 0x00E2, decomposed: &[0x0061, 0x0302] },
    Decomposition { composed: 0x00E3, decomposed: &[0x0061, 0x0303] },
    Decomposition { composed: 0x00E4, decomposed: &[0x0061, 0x0308] },
    Decomposition { composed: 0x00E5, decomposed: &[0x0061, 0x030A] },
    Decomposition { composed: 0x00E7, decomposed: &[0x0063, 0x0327] },
    Decomposition { composed: 0x00E8, decomposed: &[0x0065, 0x0300] },
    Decomposition { composed: 0x00E9, decomposed: &[0x0065, 0x0301] },
    Decomposition { composed: 0x00EA, decomposed: &[0x0065, 0x0302] },
    Decomposition { composed: 0x00EB, decomposed: &[0x0065, 0x0308] },
    Decomposition { composed: 0x00EC, decomposed: &[0x0069, 0x0300] },
    Decomposition { composed: 0x00ED, decomposed: &[0x0069, 0x0301] },
    Decomposition { composed: 0x00EE, decomposed: &[0x0069, 0x0302] },
    Decomposition { composed: 0x00EF, decomposed: &[0x0069, 0x0308] },
    Decomposition { composed: 0x00F1, decomposed: &[0x006E, 0x0303] },
    Decomposition { composed: 0x00F2, decomposed: &[0x006F, 0x0300] },
    Decomposition { composed: 0x00F3, decomposed: &[0x006F, 0x0301] },
    Decomposition { composed: 0x00F4, decomposed: &[0x006F, 0x0302] },
    Decomposition { composed: 0x00F5, decomposed: &[0x006F, 0x0303] },
    Decomposition { composed: 0x00F6, decomposed: &[0x006F, 0x0308] },
    Decomposition { composed: 0x00F9, decomposed: &[0x0075, 0x0300] },
    Decomposition { composed: 0x00FA, decomposed: &[0x0075, 0x0301] },
    Decomposition { composed: 0x00FB, decomposed: &[0x0075, 0x0302] },
    Decomposition { composed: 0x00FC, decomposed: &[0x0075, 0x0308] },
    Decomposition { composed: 0x00FD, decomposed: &[0x0079, 0x0301] },
    Decomposition { composed: 0x00FF, decomposed: &[0x0079, 0x0308] },
];

/// Look up the decomposition for a code point.
fn lookup_decomposition(cp: u32) -> Option<&'static [u32]> {
    DECOMP_TABLE
        .iter()
        .find(|d| d.composed == cp)
        .map(|d| d.decomposed)
}

/// Look up the composition for a (base, combining) pair.
fn lookup_composition(base: u32, combining: u32) -> Option<u32> {
    DECOMP_TABLE.iter().find_map(|d| {
        if d.decomposed.len() == 2
            && d.decomposed[0] == base
            && d.decomposed[1] == combining
        {
            Some(d.composed)
        } else {
            None
        }
    })
}

/// Decompose a sequence of code points into NFD form.
fn decompose(input: &[u32]) -> Vec<u32> {
    let mut result = Vec::with_capacity(input.len());
    for &cp in input {
        if let Some(decomp) = lookup_decomposition(cp) {
            result.extend_from_slice(decomp);
        } else {
            result.push(cp);
        }
    }
    result
}

/// Compose a decomposed sequence into NFC form.
fn compose(input: &[u32]) -> Vec<u32> {
    let decomposed = decompose(input);
    if decomposed.is_empty() {
        return decomposed;
    }

    let mut result = Vec::with_capacity(decomposed.len());
    result.push(decomposed[0]);

    for i in 1..decomposed.len() {
        let combining = decomposed[i];
        let base = *result.last().unwrap();

        if let Some(composed) = lookup_composition(base, combining) {
            *result.last_mut().unwrap() = composed;
        } else {
            result.push(combining);
        }
    }

    result
}

/// Normalize a Unicode string (code points) according to the given mode.
pub fn utf8_normalize(input: &[u32], mode: NormalizeMode) -> Vec<u32> {
    match mode {
        NormalizeMode::NFC | NormalizeMode::NFKC => compose(input),
        NormalizeMode::NFD | NormalizeMode::NFKD => decompose(input),
    }
}

/// Case-fold a Unicode string.
/// Simple mode: ASCII lowercase only.
/// Full mode: ASCII lowercase + Latin uppercase→lowercase.
pub fn utf8_casefold(input: &[u32], mode: CasefoldMode) -> Vec<u32> {
    let mut result = Vec::with_capacity(input.len());
    for &cp in input {
        // ASCII lowercase
        if (0x41..=0x5A).contains(&cp) {
            result.push(cp + 32);
            continue;
        }

        match mode {
            CasefoldMode::Simple => {
                result.push(cp);
            }
            CasefoldMode::Full => {
                // Latin uppercase → lowercase (À-Þ → à-þ, except ß)
                if (0x00C0..=0x00DE).contains(&cp) && cp != 0x00D7 {
                    result.push(cp + 32);
                } else {
                    result.push(cp);
                }
            }
        }
    }
    result
}

/// Compare two Unicode strings after normalization.
/// Returns 0 if equal, -1 if left < right, 1 if left > right.
pub fn normalize_cmp(left: &[u32], right: &[u32], mode: NormalizeMode) -> i32 {
    let ln = utf8_normalize(left, mode);
    let rn = utf8_normalize(right, mode);
    cmp_codepoints(&ln, &rn)
}

/// Compare two Unicode strings after case-folding.
/// Returns 0 if equal, -1 if left < right, 1 if left > right.
pub fn casefold_cmp(left: &[u32], right: &[u32], mode: CasefoldMode) -> i32 {
    let ln = utf8_casefold(left, mode);
    let rn = utf8_casefold(right, mode);
    cmp_codepoints(&ln, &rn)
}

/// Compare two code point sequences.
fn cmp_codepoints(left: &[u32], right: &[u32]) -> i32 {
    let min_len = core::cmp::min(left.len(), right.len());
    for i in 0..min_len {
        if left[i] < right[i] {
            return -1;
        }
        if left[i] > right[i] {
            return 1;
        }
    }
    match left.len().cmp(&right.len()) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

// ── Legacy API wrappers ───────────────────────────────────────────────────

/// Normalize a Unicode string (NFC form). Legacy API.
pub fn normalize_nfc(input: &[u32]) -> Vec<u32> {
    utf8_normalize(input, NormalizeMode::NFC)
}

/// Normalize a Unicode string (NFKC form). Legacy API.
pub fn normalize_nfkc(input: &[u32]) -> Vec<u32> {
    utf8_normalize(input, NormalizeMode::NFKC)
}

/// Case-fold a Unicode string. Legacy API.
pub fn case_fold(input: &[u32], uppercase: bool) -> Vec<u32> {
    if uppercase {
        // Convert lowercase to uppercase (ASCII + Latin)
        let mut result = Vec::with_capacity(input.len());
        for &cp in input {
            if (0x61..=0x7A).contains(&cp) {
                result.push(cp - 32);
            } else if (0x00E0..=0x00FE).contains(&cp) && cp != 0x00F7 {
                result.push(cp - 32);
            } else {
                result.push(cp);
            }
        }
        result
    } else {
        utf8_casefold(input, CasefoldMode::Full)
    }
}

/// Compare two Unicode strings with optional normalization. Legacy API.
pub fn unicode_compare(left: &[u32], right: &[u32], normalize: bool) -> i32 {
    if normalize {
        normalize_cmp(left, right, NormalizeMode::NFC)
    } else {
        cmp_codepoints(left, right)
    }
}
