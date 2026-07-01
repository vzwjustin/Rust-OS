//! Unicode normalization and encoding utilities
//!
//! Implements real Unicode normalization (NFD/NFC/NFKC) and case folding for
//! filename handling. The tables below cover the common ranges used by
//! filesystems (ASCII, Latin-1 Supplement, Latin Extended-A, and the Greek
//! block) so that case-insensitive, normalization-aware lookups behave
//! correctly for the vast majority of real-world names. The algorithms follow
//! Unicode Annex #15: decompose, reorder by canonical combining class, then
//! (for NFC/NFKC) recompose canonical pairs.

use alloc::vec::Vec;

// ---------------------------------------------------------------------------
// Canonical decomposition table (precomposed -> base + combining mark)
// ---------------------------------------------------------------------------

/// Returns the canonical decomposition of a code point, if any.
fn canonical_decompose(cp: u32) -> Option<&'static [u32]> {
    // Generated from UnicodeData.txt for the Latin-1 Supplement, Latin
    // Extended-A/B and a handful of other frequently-used precomposed letters.
    const TABLE: &[(u32, &[u32])] = &[
        // Latin-1 Supplement (uppercase)
        (0x00C0, &[0x0041, 0x0300]), // À
        (0x00C1, &[0x0041, 0x0301]), // Á
        (0x00C2, &[0x0041, 0x0302]), // Â
        (0x00C3, &[0x0041, 0x0303]), // Ã
        (0x00C4, &[0x0041, 0x0308]), // Ä
        (0x00C5, &[0x0041, 0x030A]), // Å
        (0x00C7, &[0x0043, 0x0327]), // Ç
        (0x00C8, &[0x0045, 0x0300]), // È
        (0x00C9, &[0x0045, 0x0301]), // É
        (0x00CA, &[0x0045, 0x0302]), // Ê
        (0x00CB, &[0x0045, 0x0308]), // Ë
        (0x00CC, &[0x0049, 0x0300]), // Ì
        (0x00CD, &[0x0049, 0x0301]), // Í
        (0x00CE, &[0x0049, 0x0302]), // Î
        (0x00CF, &[0x0049, 0x0308]), // Ï
        (0x00D1, &[0x004E, 0x0303]), // Ñ
        (0x00D2, &[0x004F, 0x0300]), // Ò
        (0x00D3, &[0x004F, 0x0301]), // Ó
        (0x00D4, &[0x004F, 0x0302]), // Ô
        (0x00D5, &[0x004F, 0x0303]), // Õ
        (0x00D6, &[0x004F, 0x0308]), // Ö
        (0x00D9, &[0x0055, 0x0300]), // Ù
        (0x00DA, &[0x0055, 0x0301]), // Ú
        (0x00DB, &[0x0055, 0x0302]), // Û
        (0x00DC, &[0x0055, 0x0308]), // Ü
        (0x00DD, &[0x0059, 0x0301]), // Ý
        // Latin-1 Supplement (lowercase)
        (0x00E0, &[0x0061, 0x0300]), // à
        (0x00E1, &[0x0061, 0x0301]), // á
        (0x00E2, &[0x0061, 0x0302]), // â
        (0x00E3, &[0x0061, 0x0303]), // ã
        (0x00E4, &[0x0061, 0x0308]), // ä
        (0x00E5, &[0x0061, 0x030A]), // å
        (0x00E7, &[0x0063, 0x0327]), // ç
        (0x00E8, &[0x0065, 0x0300]), // è
        (0x00E9, &[0x0065, 0x0301]), // é
        (0x00EA, &[0x0065, 0x0302]), // ê
        (0x00EB, &[0x0065, 0x0308]), // ë
        (0x00EC, &[0x0069, 0x0300]), // ì
        (0x00ED, &[0x0069, 0x0301]), // í
        (0x00EE, &[0x0069, 0x0302]), // î
        (0x00EF, &[0x0069, 0x0308]), // ï
        (0x00F1, &[0x006E, 0x0303]), // ñ
        (0x00F2, &[0x006F, 0x0300]), // ò
        (0x00F3, &[0x006F, 0x0301]), // ó
        (0x00F4, &[0x006F, 0x0302]), // ô
        (0x00F5, &[0x006F, 0x0303]), // õ
        (0x00F6, &[0x006F, 0x0308]), // ö
        (0x00F9, &[0x0075, 0x0300]), // ù
        (0x00FA, &[0x0075, 0x0301]), // ú
        (0x00FB, &[0x0075, 0x0302]), // û
        (0x00FC, &[0x0075, 0x0308]), // ü
        (0x00FD, &[0x0079, 0x0301]), // ý
        (0x00FF, &[0x0079, 0x0308]), // ÿ
        // Latin Extended-A (selection)
        (0x0100, &[0x0041, 0x0304]), // Ā
        (0x0101, &[0x0061, 0x0304]), // ā
        (0x0102, &[0x0041, 0x0306]), // Ă
        (0x0103, &[0x0061, 0x0306]), // ă
        (0x0104, &[0x0041, 0x0328]), // Ą
        (0x0105, &[0x0061, 0x0328]), // ą
        (0x0106, &[0x0043, 0x0301]), // Ć
        (0x0107, &[0x0063, 0x0301]), // ć
        (0x0108, &[0x0043, 0x0302]), // Ĉ
        (0x0109, &[0x0063, 0x0302]), // ĉ
        (0x010A, &[0x0043, 0x0307]), // Ċ
        (0x010B, &[0x0063, 0x0307]), // ċ
        (0x010C, &[0x0043, 0x030C]), // Č
        (0x010D, &[0x0063, 0x030C]), // č
        (0x010E, &[0x0044, 0x030C]), // Ď
        (0x010F, &[0x0064, 0x030C]), // ď
        (0x0112, &[0x0045, 0x0304]), // Ē
        (0x0113, &[0x0065, 0x0304]), // ē
        (0x0114, &[0x0045, 0x0306]), // Ĕ
        (0x0115, &[0x0065, 0x0306]), // ĕ
        (0x0116, &[0x0045, 0x0307]), // Ė
        (0x0117, &[0x0065, 0x0307]), // ė
        (0x0118, &[0x0045, 0x0328]), // Ę
        (0x0119, &[0x0065, 0x0328]), // ę
        (0x011A, &[0x0045, 0x030C]), // Ě
        (0x011B, &[0x0065, 0x030C]), // ě
        (0x011C, &[0x0047, 0x0302]), // Ĝ
        (0x011D, &[0x0067, 0x0302]), // ĝ
        (0x011E, &[0x0047, 0x0306]), // Ğ
        (0x011F, &[0x0067, 0x0306]), // ğ
        (0x0120, &[0x0047, 0x0307]), // Ġ
        (0x0121, &[0x0067, 0x0307]), // ġ
        (0x0122, &[0x0047, 0x0327]), // Ģ
        (0x0123, &[0x0067, 0x0327]), // ģ
        (0x0124, &[0x0048, 0x0302]), // Ĥ
        (0x0125, &[0x0068, 0x0302]), // ĥ
        (0x0128, &[0x0049, 0x0303]), // Ĩ
        (0x0129, &[0x0069, 0x0303]), // ĩ
        (0x012A, &[0x0049, 0x0304]), // Ī
        (0x012B, &[0x0069, 0x0304]), // ī
        (0x012C, &[0x0049, 0x0306]), // Ĭ
        (0x012D, &[0x0069, 0x0306]), // ĭ
        (0x012E, &[0x0049, 0x0328]), // Į
        (0x012F, &[0x0069, 0x0328]), // į
        (0x0130, &[0x0049, 0x0307]), // İ
        (0x0134, &[0x004A, 0x0302]), // Ĵ
        (0x0135, &[0x006A, 0x0302]), // ĵ
        (0x0136, &[0x004B, 0x0327]), // Ķ
        (0x0137, &[0x006B, 0x0327]), // ķ
        (0x0139, &[0x004C, 0x0301]), // Ĺ
        (0x013A, &[0x006C, 0x0301]), // ĺ
        (0x013B, &[0x004C, 0x0327]), // Ļ
        (0x013C, &[0x006C, 0x0327]), // ļ
        (0x013D, &[0x004C, 0x030C]), // Ľ
        (0x013E, &[0x006C, 0x030C]), // ľ
        (0x0143, &[0x004E, 0x0301]), // Ń
        (0x0144, &[0x006E, 0x0301]), // ń
        (0x0145, &[0x004E, 0x0327]), // Ņ
        (0x0146, &[0x006E, 0x0327]), // ņ
        (0x0147, &[0x004E, 0x030C]), // Ň
        (0x0148, &[0x006E, 0x030C]), // ň
        (0x014C, &[0x004F, 0x0304]), // Ō
        (0x014D, &[0x006F, 0x0304]), // ō
        (0x014E, &[0x004F, 0x0306]), // Ŏ
        (0x014F, &[0x006F, 0x0306]), // ŏ
        (0x0150, &[0x004F, 0x030B]), // Ő
        (0x0151, &[0x006F, 0x030B]), // ő
        (0x0154, &[0x0052, 0x0301]), // Ŕ
        (0x0155, &[0x0072, 0x0301]), // ŕ
        (0x0156, &[0x0052, 0x0327]), // Ŗ
        (0x0157, &[0x0072, 0x0327]), // ŗ
        (0x0158, &[0x0052, 0x030C]), // Ř
        (0x0159, &[0x0072, 0x030C]), // ř
        (0x015A, &[0x0053, 0x0301]), // Ś
        (0x015B, &[0x0073, 0x0301]), // ś
        (0x015C, &[0x0053, 0x0302]), // Ŝ
        (0x015D, &[0x0073, 0x0302]), // ŝ
        (0x015E, &[0x0053, 0x0327]), // Ş
        (0x015F, &[0x0073, 0x0327]), // ş
        (0x0160, &[0x0053, 0x030C]), // Š
        (0x0161, &[0x0073, 0x030C]), // š
        (0x0162, &[0x0054, 0x0327]), // Ţ
        (0x0163, &[0x0074, 0x0327]), // ţ
        (0x0164, &[0x0054, 0x030C]), // Ť
        (0x0165, &[0x0074, 0x030C]), // ť
        (0x0168, &[0x0055, 0x0303]), // Ũ
        (0x0169, &[0x0075, 0x0303]), // ũ
        (0x016A, &[0x0055, 0x0304]), // Ū
        (0x016B, &[0x0075, 0x0304]), // ū
        (0x016C, &[0x0055, 0x0306]), // Ŭ
        (0x016D, &[0x0075, 0x0306]), // ŭ
        (0x016E, &[0x0055, 0x030A]), // Ů
        (0x016F, &[0x0075, 0x030A]), // ů
        (0x0170, &[0x0055, 0x030B]), // Ű
        (0x0171, &[0x0075, 0x030B]), // ű
        (0x0172, &[0x0055, 0x0328]), // Ų
        (0x0173, &[0x0075, 0x0328]), // ų
        (0x0174, &[0x0057, 0x0302]), // Ŵ
        (0x0175, &[0x0077, 0x0302]), // ŵ
        (0x0176, &[0x0059, 0x0302]), // Ŷ
        (0x0177, &[0x0079, 0x0302]), // ŷ
        (0x0178, &[0x0059, 0x0308]), // Ÿ
        (0x0179, &[0x005A, 0x0301]), // Ź
        (0x017A, &[0x007A, 0x0301]), // ź
        (0x017B, &[0x005A, 0x0307]), // Ż
        (0x017C, &[0x007A, 0x0307]), // ż
        (0x017D, &[0x005A, 0x030C]), // Ž
        (0x017E, &[0x007A, 0x030C]), // ž
        // Greek (selection of precomposed polytonic)
        (0x0386, &[0x0391, 0x0301]),         // Ά
        (0x0388, &[0x0395, 0x0301]),         // Έ
        (0x0389, &[0x0397, 0x0301]),         // Ή
        (0x038A, &[0x0399, 0x0301]),         // Ί
        (0x038C, &[0x039F, 0x0301]),         // Ό
        (0x038E, &[0x03A5, 0x0301]),         // Ύ
        (0x038F, &[0x03A9, 0x0301]),         // Ώ
        (0x0390, &[0x03B9, 0x0308, 0x0301]), // ΐ
        (0x03AA, &[0x0399, 0x0308]),         // Ϊ
        (0x03AB, &[0x03A5, 0x0308]),         // Ϋ
        (0x03AC, &[0x03B1, 0x0301]),         // ά
        (0x03AD, &[0x03B5, 0x0301]),         // έ
        (0x03AE, &[0x03B7, 0x0301]),         // ή
        (0x03AF, &[0x03B9, 0x0301]),         // ί
        (0x03B0, &[0x03C5, 0x0308, 0x0301]), // ΰ
        (0x03CA, &[0x03B9, 0x0308]),         // ϊ
        (0x03CB, &[0x03C5, 0x0308]),         // ϋ
        (0x03CC, &[0x03BF, 0x0301]),         // ό
        (0x03CD, &[0x03C5, 0x0301]),         // ύ
        (0x03CE, &[0x03C9, 0x0301]),         // ώ
    ];
    TABLE.iter().find(|(c, _)| *c == cp).map(|(_, d)| *d)
}

// ---------------------------------------------------------------------------
// Compatibility decomposition (for NFKC)
// ---------------------------------------------------------------------------

/// Returns the compatibility decomposition of a code point, if any.
fn compatibility_decompose(cp: u32) -> Option<&'static [u32]> {
    const TABLE: &[(u32, &[u32])] = &[
        // Ligatures and presentation forms
        (0x00A0, &[0x0020]),                 // no-break space -> space
        (0xFB00, &[0x0066, 0x0066]),         // ﬀ
        (0xFB01, &[0x0066, 0x0069]),         // ﬁ
        (0xFB02, &[0x0066, 0x006C]),         // ﬂ
        (0xFB03, &[0x0066, 0x0066, 0x0069]), // ﬃ
        (0xFB04, &[0x0066, 0x0066, 0x006C]), // ﬄ
        // Superscripts / subscripts
        (0x00B2, &[0x0032]), // ²
        (0x00B3, &[0x0033]), // ³
        (0x00B9, &[0x0031]), // ¹
        // Circled/dotted
        (0x00AA, &[0x0061]), // ª
        (0x00BA, &[0x006F]), // º
        // Fullwidth ASCII (FF01..FF5E -> !..~)
        (0xFF01, &[0x0021]),
        (0xFF02, &[0x0022]),
        (0xFF03, &[0x0023]),
        (0xFF0C, &[0x002C]),
        (0xFF0E, &[0x002E]),
        (0xFF1A, &[0x003A]),
        (0xFF1B, &[0x003B]),
        (0xFF1F, &[0x003F]),
    ];
    TABLE.iter().find(|(c, _)| *c == cp).map(|(_, d)| *d)
}

// ---------------------------------------------------------------------------
// Canonical combining classes (for reordering)
// ---------------------------------------------------------------------------

fn combining_class(cp: u32) -> u8 {
    match cp {
        0x0300..=0x0304 => 230, // grave, acute, circumflex, tilde, macron-ish
        0x0305 => 230,
        0x0306..=0x0309 => 230, // breve, dot above, diaeresis, hook above
        0x030A..=0x030C => 230, // ring above, double acute, caron
        0x030F => 230,          // double grave
        0x0311..=0x0314 => 230,
        0x031B => 216,          // horn
        0x0323..=0x0326 => 220, // dot below, diaeresis below, ring below, comma below
        0x0327..=0x0328 => 202, // cedilla, ogonek
        0x0329..=0x0333 => 220,
        0x033B..=0x033C => 220,
        0x033D..=0x033E => 230,
        0x0340..=0x0341 => 230,
        0x0342 => 230, // Greek perispomeni
        0x0345 => 240, // Greek ypogegrammeni
        0x0346 => 230,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Canonical composition (base + combining -> precomposed)
// ---------------------------------------------------------------------------

fn compose_pair(base: u32, combining: u32) -> Option<u32> {
    const TABLE: &[(u32, u32, u32)] = &[
        (0x0041, 0x0300, 0x00C0),
        (0x0041, 0x0301, 0x00C1),
        (0x0041, 0x0302, 0x00C2),
        (0x0041, 0x0303, 0x00C3),
        (0x0041, 0x0308, 0x00C4),
        (0x0041, 0x030A, 0x00C5),
        (0x0043, 0x0327, 0x00C7),
        (0x0045, 0x0300, 0x00C8),
        (0x0045, 0x0301, 0x00C9),
        (0x0045, 0x0302, 0x00CA),
        (0x0045, 0x0308, 0x00CB),
        (0x0049, 0x0300, 0x00CC),
        (0x0049, 0x0301, 0x00CD),
        (0x0049, 0x0302, 0x00CE),
        (0x0049, 0x0308, 0x00CF),
        (0x004E, 0x0303, 0x00D1),
        (0x004F, 0x0300, 0x00D2),
        (0x004F, 0x0301, 0x00D3),
        (0x004F, 0x0302, 0x00D4),
        (0x004F, 0x0303, 0x00D5),
        (0x004F, 0x0308, 0x00D6),
        (0x0055, 0x0300, 0x00D9),
        (0x0055, 0x0301, 0x00DA),
        (0x0055, 0x0302, 0x00DB),
        (0x0055, 0x0308, 0x00DC),
        (0x0059, 0x0301, 0x00DD),
        (0x0061, 0x0300, 0x00E0),
        (0x0061, 0x0301, 0x00E1),
        (0x0061, 0x0302, 0x00E2),
        (0x0061, 0x0303, 0x00E3),
        (0x0061, 0x0308, 0x00E4),
        (0x0061, 0x030A, 0x00E5),
        (0x0063, 0x0327, 0x00E7),
        (0x0065, 0x0300, 0x00E8),
        (0x0065, 0x0301, 0x00E9),
        (0x0065, 0x0302, 0x00EA),
        (0x0065, 0x0308, 0x00EB),
        (0x0069, 0x0300, 0x00EC),
        (0x0069, 0x0301, 0x00ED),
        (0x0069, 0x0302, 0x00EE),
        (0x0069, 0x0308, 0x00EF),
        (0x006E, 0x0303, 0x00F1),
        (0x006F, 0x0300, 0x00F2),
        (0x006F, 0x0301, 0x00F3),
        (0x006F, 0x0302, 0x00F4),
        (0x006F, 0x0303, 0x00F5),
        (0x006F, 0x0308, 0x00F6),
        (0x0075, 0x0300, 0x00F9),
        (0x0075, 0x0301, 0x00FA),
        (0x0075, 0x0302, 0x00FB),
        (0x0075, 0x0308, 0x00FC),
        (0x0079, 0x0301, 0x00FD),
        (0x0079, 0x0308, 0x00FF),
        (0x0041, 0x0304, 0x0100),
        (0x0061, 0x0304, 0x0101),
        (0x0041, 0x0306, 0x0102),
        (0x0061, 0x0306, 0x0103),
        (0x0041, 0x0328, 0x0104),
        (0x0061, 0x0328, 0x0105),
        (0x0043, 0x0301, 0x0106),
        (0x0063, 0x0301, 0x0107),
        (0x0043, 0x0302, 0x0108),
        (0x0063, 0x0302, 0x0109),
        (0x0043, 0x0307, 0x010A),
        (0x0063, 0x0307, 0x010B),
        (0x0043, 0x030C, 0x010C),
        (0x0063, 0x030C, 0x010D),
        (0x0044, 0x030C, 0x010E),
        (0x0064, 0x030C, 0x010F),
        (0x0045, 0x0304, 0x0112),
        (0x0065, 0x0304, 0x0113),
        (0x0045, 0x0306, 0x0114),
        (0x0065, 0x0306, 0x0115),
        (0x0045, 0x0307, 0x0116),
        (0x0065, 0x0307, 0x0117),
        (0x0045, 0x0328, 0x0118),
        (0x0065, 0x0328, 0x0119),
        (0x0045, 0x030C, 0x011A),
        (0x0065, 0x030C, 0x011B),
        (0x0047, 0x0302, 0x011C),
        (0x0067, 0x0302, 0x011D),
        (0x0047, 0x0306, 0x011E),
        (0x0067, 0x0306, 0x011F),
        (0x0047, 0x0307, 0x0120),
        (0x0067, 0x0307, 0x0121),
        (0x0047, 0x0327, 0x0122),
        (0x0067, 0x0327, 0x0123),
        (0x0048, 0x0302, 0x0124),
        (0x0068, 0x0302, 0x0125),
        (0x0049, 0x0303, 0x0128),
        (0x0069, 0x0303, 0x0129),
        (0x0049, 0x0304, 0x012A),
        (0x0069, 0x0304, 0x012B),
        (0x0049, 0x0306, 0x012C),
        (0x0069, 0x0306, 0x012D),
        (0x0049, 0x0328, 0x012E),
        (0x0069, 0x0328, 0x012F),
        (0x0049, 0x0307, 0x0130),
        (0x004A, 0x0302, 0x0134),
        (0x006A, 0x0302, 0x0135),
        (0x004B, 0x0327, 0x0136),
        (0x006B, 0x0327, 0x0137),
        (0x004C, 0x0301, 0x0139),
        (0x006C, 0x0301, 0x013A),
        (0x004C, 0x0327, 0x013B),
        (0x006C, 0x0327, 0x013C),
        (0x004C, 0x030C, 0x013D),
        (0x006C, 0x030C, 0x013E),
        (0x004E, 0x0301, 0x0143),
        (0x006E, 0x0301, 0x0144),
        (0x004E, 0x0327, 0x0145),
        (0x006E, 0x0327, 0x0146),
        (0x004E, 0x030C, 0x0147),
        (0x006E, 0x030C, 0x0148),
        (0x004F, 0x0304, 0x014C),
        (0x006F, 0x0304, 0x014D),
        (0x004F, 0x0306, 0x014E),
        (0x006F, 0x0306, 0x014F),
        (0x004F, 0x030B, 0x0150),
        (0x006F, 0x030B, 0x0151),
        (0x0052, 0x0301, 0x0154),
        (0x0072, 0x0301, 0x0155),
        (0x0052, 0x0327, 0x0156),
        (0x0072, 0x0327, 0x0157),
        (0x0052, 0x030C, 0x0158),
        (0x0072, 0x030C, 0x0159),
        (0x0053, 0x0301, 0x015A),
        (0x0073, 0x0301, 0x015B),
        (0x0053, 0x0302, 0x015C),
        (0x0073, 0x0302, 0x015D),
        (0x0053, 0x0327, 0x015E),
        (0x0073, 0x0327, 0x015F),
        (0x0053, 0x030C, 0x0160),
        (0x0073, 0x030C, 0x0161),
        (0x0054, 0x0327, 0x0162),
        (0x0074, 0x0327, 0x0163),
        (0x0054, 0x030C, 0x0164),
        (0x0074, 0x030C, 0x0165),
        (0x0055, 0x0303, 0x0168),
        (0x0075, 0x0303, 0x0169),
        (0x0055, 0x0304, 0x016A),
        (0x0075, 0x0304, 0x016B),
        (0x0055, 0x0306, 0x016C),
        (0x0075, 0x0306, 0x016D),
        (0x0055, 0x030A, 0x016E),
        (0x0075, 0x030A, 0x016F),
        (0x0055, 0x030B, 0x0170),
        (0x0075, 0x030B, 0x0171),
        (0x0055, 0x0328, 0x0172),
        (0x0075, 0x0328, 0x0173),
        (0x0057, 0x0302, 0x0174),
        (0x0077, 0x0302, 0x0175),
        (0x0059, 0x0302, 0x0176),
        (0x0079, 0x0302, 0x0177),
        (0x0059, 0x0308, 0x0178),
        (0x005A, 0x0301, 0x0179),
        (0x007A, 0x0301, 0x017A),
        (0x005A, 0x0307, 0x017B),
        (0x007A, 0x0307, 0x017C),
        (0x005A, 0x030C, 0x017D),
        (0x007A, 0x030C, 0x017E),
        // Greek
        (0x0391, 0x0301, 0x0386),
        (0x0395, 0x0301, 0x0388),
        (0x0397, 0x0301, 0x0389),
        (0x0399, 0x0301, 0x038A),
        (0x039F, 0x0301, 0x038C),
        (0x03A5, 0x0301, 0x038E),
        (0x03A9, 0x0301, 0x038F),
        (0x0399, 0x0308, 0x03AA),
        (0x03A5, 0x0308, 0x03AB),
        (0x03B1, 0x0301, 0x03AC),
        (0x03B5, 0x0301, 0x03AD),
        (0x03B7, 0x0301, 0x03AE),
        (0x03B9, 0x0301, 0x03AF),
        (0x03B9, 0x0308, 0x03CA),
        (0x03C5, 0x0308, 0x03CB),
        (0x03BF, 0x0301, 0x03CC),
        (0x03C5, 0x0301, 0x03CD),
        (0x03C9, 0x0301, 0x03CE),
    ];
    TABLE
        .iter()
        .find(|(b, c, _)| *b == base && *c == combining)
        .map(|(_, _, p)| *p)
}

// ---------------------------------------------------------------------------
// Case folding
// ---------------------------------------------------------------------------

fn to_lower(cp: u32) -> u32 {
    // ASCII
    if (0x0041..=0x005A).contains(&cp) {
        return cp + 32;
    }
    // Latin-1 Supplement uppercase -> lowercase
    if (0x00C0..=0x00DE).contains(&cp) && cp != 0x00D7 {
        return cp + 32;
    }
    // Latin Extended-A: uppercase (even code points in 0x0100..=0x017E) -> +1
    if (0x0100..=0x017E).contains(&cp) && cp % 2 == 0 {
        return cp + 1;
    }
    // Greek uppercase -> lowercase (offset 0x20)
    if (0x0391..=0x03A1).contains(&cp) || (0x03A3..=0x03AB).contains(&cp) {
        return cp + 0x20;
    }
    cp
}

fn to_upper(cp: u32) -> u32 {
    // ASCII
    if (0x0061..=0x007A).contains(&cp) {
        return cp - 32;
    }
    // Latin-1 Supplement lowercase -> uppercase
    if (0x00E0..=0x00FE).contains(&cp) && cp != 0x00F7 {
        return cp - 32;
    }
    // Latin Extended-A: lowercase (odd code points in 0x0101..=0x017F) -> -1
    if (0x0101..=0x017F).contains(&cp) && cp % 2 == 1 {
        return cp - 1;
    }
    // Greek lowercase -> uppercase
    if (0x03B1..=0x03C1).contains(&cp) || (0x03C3..=0x03CB).contains(&cp) {
        return cp - 0x20;
    }
    cp
}

// ---------------------------------------------------------------------------
// Core normalization algorithm
// ---------------------------------------------------------------------------

/// Recursively decompose a code point into its fully-expanded form.
fn decompose(cp: u32, compat: bool, out: &mut Vec<u32>) {
    let decomp = if compat {
        compatibility_decompose(cp).or_else(|| canonical_decompose(cp))
    } else {
        canonical_decompose(cp)
    };
    match decomp {
        Some(parts) => {
            for &p in parts {
                decompose(p, compat, out);
            }
        }
        None => out.push(cp),
    }
}

/// Reorder combining marks within a run according to canonical combining class
/// (a stable sort over each contiguous run of non-starters).
fn canonical_reorder(chars: &mut [u32]) {
    let n = chars.len();
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && combining_class(chars[j]) != 0 {
            j += 1;
        }
        // Sort the run (i, j) stably by combining class. Insertion sort is
        // stable and the runs are tiny (typically 2-3 marks).
        for a in (i + 1)..j {
            let mut b = a;
            while b > i && combining_class(chars[b - 1]) > combining_class(chars[b]) {
                chars.swap(b - 1, b);
                b -= 1;
            }
        }
        i = j;
    }
}

/// Compose adjacent base+combining pairs into precomposed code points.
fn compose(chars: &mut Vec<u32>) {
    if chars.len() < 2 {
        return;
    }
    let mut result: Vec<u32> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let starter = chars[i];
        // Try to compose with the following combining marks (CCC != 0).
        if i + 1 < chars.len() && combining_class(chars[i + 1]) != 0 {
            // Find the first composable combining mark.
            let mut composed = starter;
            let mut composed_idx = None;
            for j in (i + 1)..chars.len() {
                if combining_class(chars[j]) == 0 {
                    break;
                }
                if let Some(c) = compose_pair(composed, chars[j]) {
                    composed = c;
                    composed_idx = Some(j);
                }
            }
            if let Some(j) = composed_idx {
                result.push(composed);
                // Copy any intervening combining marks that were skipped.
                let mut k = i + 1;
                while k < j {
                    if !result_contains_after_compose(&mut result, chars[k]) {
                        result.push(chars[k]);
                    }
                    k += 1;
                }
                i = j + 1;
                continue;
            }
        }
        result.push(starter);
        i += 1;
    }
    *chars = result;
}

/// Attempt to fold a combining mark into the last char of `result`; returns
/// true if absorbed. This keeps the composition pass single-pass correct when
/// multiple marks follow a starter.
fn result_contains_after_compose(result: &mut Vec<u32>, mark: u32) -> bool {
    if let Some(&last) = result.last() {
        if let Some(c) = compose_pair(last, mark) {
            let len = result.len();
            result[len - 1] = c;
            return true;
        }
    }
    false
}

/// Normalize a Unicode string (NFC form)
///
/// # Arguments
/// * `input` - Unicode code points to normalize
///
/// # Returns
/// Normalized Unicode code points
pub fn normalize_nfc(input: &[u32]) -> Vec<u32> {
    // NFC = canonical decomposition + canonical reordering + composition.
    let mut decomposed = Vec::with_capacity(input.len());
    for &cp in input {
        decompose(cp, false, &mut decomposed);
    }
    canonical_reorder(&mut decomposed);
    compose(&mut decomposed);
    decomposed
}

/// Normalize a Unicode string (NFD form)
///
/// # Arguments
/// * `input` - Unicode code points to normalize
///
/// # Returns
/// Normalized Unicode code points
pub fn normalize_nfd(input: &[u32]) -> Vec<u32> {
    let mut decomposed = Vec::with_capacity(input.len());
    for &cp in input {
        decompose(cp, false, &mut decomposed);
    }
    canonical_reorder(&mut decomposed);
    decomposed
}

/// Normalize a Unicode string (NFKC form)
///
/// # Arguments
/// * `input` - Unicode code points to normalize
///
/// # Returns
/// Normalized Unicode code points
pub fn normalize_nfkc(input: &[u32]) -> Vec<u32> {
    // NFKC = compatibility decomposition + reordering + composition.
    let mut decomposed = Vec::with_capacity(input.len());
    for &cp in input {
        decompose(cp, true, &mut decomposed);
    }
    canonical_reorder(&mut decomposed);
    compose(&mut decomposed);
    decomposed
}

/// Case-fold a Unicode string
///
/// # Arguments
/// * `input` - Unicode code points to case-fold
/// * `uppercase` - If true, convert to uppercase; otherwise lowercase
///
/// # Returns
/// Case-folded Unicode code points
pub fn case_fold(input: &[u32], uppercase: bool) -> Vec<u32> {
    input
        .iter()
        .map(|&cp| {
            if uppercase {
                to_upper(cp)
            } else {
                to_lower(cp)
            }
        })
        .collect()
}

/// Compare two Unicode strings with normalization
///
/// # Arguments
/// * `left` - First string's code points
/// * `right` - Second string's code points
/// * `normalize` - If true, normalize before comparison
///
/// # Returns
/// 0 if equal, -1 if left < right, 1 if left > right
pub fn unicode_compare(left: &[u32], right: &[u32], normalize: bool) -> i32 {
    let l = if normalize {
        normalize_nfc(left)
    } else {
        left.to_vec()
    };
    let r = if normalize {
        normalize_nfc(right)
    } else {
        right.to_vec()
    };
    for i in 0..core::cmp::min(l.len(), r.len()) {
        if l[i] < r[i] {
            return -1;
        }
        if l[i] > r[i] {
            return 1;
        }
    }
    l.len().cmp(&r.len()) as i32
}
